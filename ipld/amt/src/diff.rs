// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use serde::{de::DeserializeOwned, Serialize};

use crate::node::CollapsedNode;

use super::*;

pub enum ChangeType {
    Add,
    Remove,
    Modify,
}

pub struct Change<V> {
    pub change_type: ChangeType,
    pub key: u64,
    pub before: Option<V>,
    pub after: Option<V>,
}

struct NodeContext {
    pub height: u32,
    pub bit_width: u32,
}

impl NodeContext {
    fn nodes_at_height(&self) -> u64 {
        nodes_for_height(self.bit_width, self.height)
    }
}

impl<V, BS> From<&Amt<V, BS>> for NodeContext {
    fn from(value: &Amt<V, BS>) -> Self {
        Self {
            height: value.height(),
            bit_width: value.bit_width(),
        }
    }
}

pub fn diff<V, BS>(prev_amt: &Amt<V, BS>, curr_amt: &Amt<V, BS>) -> anyhow::Result<Vec<Change<V>>>
where
    V: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    if prev_amt.bit_width() != curr_amt.bit_width() {
        anyhow::bail!(
            "diffing AMTs with differing bitWidths not supported (prev={}, cur={})",
            prev_amt.bit_width(),
            curr_amt.bit_width()
        );
    }

    if prev_amt.count() == 0 && curr_amt.count() != 0 {
        add_all(
            &curr_amt.block_store,
            &curr_amt.into(),
            &curr_amt.root.node,
            0,
        )
    } else if prev_amt.count() != 0 && curr_amt.count() == 0 {
        remove_all(
            &prev_amt.block_store,
            &prev_amt.into(),
            &prev_amt.root.node,
            0,
        )
    } else {
        diff_node(
            &curr_amt.block_store,
            &prev_amt.into(),
            &prev_amt.root.node,
            &curr_amt.into(),
            &curr_amt.root.node,
            0,
        )
    }
}

fn add_all<V, BS>(
    store: &BS,
    ctx: &NodeContext,
    node: &Node<V>,
    offset: u64,
) -> anyhow::Result<Vec<Change<V>>>
where
    V: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    let mut changes = vec![];
    node.for_each_while(store, ctx.height, ctx.bit_width, offset, &mut |i, x| {
        changes.push(Change {
            change_type: ChangeType::Add,
            key: i,
            before: None,
            after: Some(x.clone()),
        });
        Ok(true)
    })?;

    Ok(changes)
}

fn remove_all<V, BS>(
    store: &BS,
    ctx: &NodeContext,
    node: &Node<V>,
    offset: u64,
) -> anyhow::Result<Vec<Change<V>>>
where
    V: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    let mut changes = vec![];
    node.for_each_while(store, ctx.height, ctx.bit_width, offset, &mut |i, x| {
        changes.push(Change {
            change_type: ChangeType::Remove,
            key: i,
            before: None,
            after: Some(x.clone()),
        });
        Ok(true)
    })?;

    Ok(changes)
}

fn diff_leaves<V>(
    prev_node: &Node<V>,
    curr_node: &Node<V>,
    offset: u64,
) -> anyhow::Result<Vec<Change<V>>>
where
    V: Serialize + DeserializeOwned + Clone,
{
    let prev_vals = match prev_node {
        Node::Leaf { vals } => vals,
        _ => {
            anyhow::bail!("Leaf expected")
        }
    };

    let curr_vals = match curr_node {
        Node::Leaf { vals } => vals,
        _ => {
            anyhow::bail!("Leaf expected")
        }
    };

    anyhow::ensure!(
        prev_vals.len() == curr_vals.len(),
        "node leaves have different numbers of values"
    );

    let mut changes = vec![];

    for (i, (prev_val, curr_val)) in prev_vals.iter().zip(curr_vals.iter()).enumerate() {
        let index = offset + i as u64;
        match (prev_val, curr_val) {
            (None, None) => continue,
            (None, Some(curr_val)) => changes.push(Change {
                change_type: ChangeType::Add,
                key: index,
                before: None,
                after: Some(curr_val.clone()),
            }),
            (Some(prev_val), None) => changes.push(Change {
                change_type: ChangeType::Remove,
                key: index,
                before: Some(prev_val.clone()),
                after: None,
            }),
            (Some(prev_val), Some(curr_val)) => changes.push(Change {
                change_type: ChangeType::Modify,
                key: index,
                before: Some(prev_val.clone()),
                after: Some(curr_val.clone()),
            }),
        }
    }

    Ok(changes)
}

fn diff_node<V, BS>(
    store: &BS,
    prev_ctx: &NodeContext,
    prev_node: &Node<V>,
    curr_ctx: &NodeContext,
    curr_node: &Node<V>,
    offset: u64,
) -> anyhow::Result<Vec<Change<V>>>
where
    V: Serialize + DeserializeOwned + Clone,
    BS: Blockstore,
{
    if prev_ctx.height == 0 && curr_ctx.height == 0 {
        diff_leaves(prev_node, curr_node, offset)
    } else if curr_ctx.height > prev_ctx.height {
        let mut changes = vec![];
        let sub_count = curr_ctx.nodes_at_height();
        let links = match curr_node {
            Node::Link { links } => links,
            _ => anyhow::bail!("Node::Link expected"),
        };
        for (i, link) in links.iter().enumerate() {
            if let Some(link) = link {
                let sub_node = match link {
                    node::Link::Cid { cid, .. } => store
                        .get_cbor::<CollapsedNode<V>>(cid)?
                        .context("Failed to get collapsed node from block store")?
                        .expand(curr_ctx.bit_width)?,
                    _ => {
                        anyhow::bail!("Unchanged link expected")
                    }
                };
                let new_offset = offset + sub_count * i as u64;
                let sub_ctx = NodeContext {
                    height: curr_ctx.height - 1,
                    bit_width: curr_ctx.bit_width,
                };

                changes.append(&mut if i == 0 {
                    diff_node(store, prev_ctx, prev_node, &sub_ctx, &sub_node, new_offset)?
                } else {
                    add_all(store, &sub_ctx, &sub_node, new_offset)?
                });
            }
        }

        Ok(changes)
    } else if curr_ctx.height < prev_ctx.height {
        let mut changes = vec![];
        let sub_count = nodes_for_height(prev_ctx.bit_width, prev_ctx.height);
        let links = match prev_node {
            Node::Link { links } => links,
            _ => anyhow::bail!("Node::Link expected"),
        };
        for (i, link) in links.iter().enumerate() {
            if let Some(link) = link {
                let sub_node = match link {
                    node::Link::Cid { cid, .. } => store
                        .get_cbor::<CollapsedNode<V>>(cid)?
                        .context("Failed to get collapsed node from block store")?
                        .expand(prev_ctx.bit_width)?,
                    _ => {
                        anyhow::bail!("Unchanged link expected")
                    }
                };
                let new_offset = offset + sub_count * i as u64;
                let sub_ctx = NodeContext {
                    height: prev_ctx.height - 1,
                    bit_width: prev_ctx.bit_width,
                };

                changes.append(&mut if i == 0 {
                    diff_node(store, &sub_ctx, &sub_node, curr_ctx, curr_node, new_offset)?
                } else {
                    remove_all(store, &sub_ctx, &sub_node, new_offset)?
                });
            }
        }

        Ok(changes)
    } else {
        anyhow::ensure!(
            prev_ctx.height == curr_ctx.height,
            "comparing non-leaf nodes of unequal heights"
        );

        match (prev_node, curr_node) {
            (Node::Link { links: prev_links }, Node::Link { links: curr_links }) => {
                anyhow::ensure!(
                    prev_links.len() == curr_links.len(),
                    "nodes have different numbers of links"
                );

                let mut changes = vec![];
                let sub_count = prev_ctx.nodes_at_height();

                for (i, (prev_link, curr_link)) in
                    prev_links.iter().zip(curr_links.iter()).enumerate()
                {
                    match (prev_link, curr_link) {
                        (None, None) => continue,
                        (Some(prev_link), None) => {
                            let sub_ctx = NodeContext {
                                bit_width: prev_ctx.bit_width,
                                height: prev_ctx.height - 1,
                            };
                            let sub_node = match prev_link {
                                node::Link::Cid { cid, .. } => store
                                    .get_cbor::<CollapsedNode<V>>(cid)?
                                    .context("Failed to get collapsed node from block store")?
                                    .expand(prev_ctx.bit_width)?,
                                _ => {
                                    anyhow::bail!("Unchanged link expected")
                                }
                            };
                            let new_offset = offset + sub_count * i as u64;
                            changes
                                .append(&mut remove_all(store, &sub_ctx, &sub_node, new_offset)?);
                        }
                        (None, Some(curr_link)) => {
                            let sub_ctx = NodeContext {
                                bit_width: curr_ctx.bit_width,
                                height: curr_ctx.height - 1,
                            };
                            let sub_node = match curr_link {
                                node::Link::Cid { cid, .. } => store
                                    .get_cbor::<CollapsedNode<V>>(cid)?
                                    .context("Failed to get collapsed node from block store")?
                                    .expand(curr_ctx.bit_width)?,
                                _ => {
                                    anyhow::bail!("Unchanged link expected")
                                }
                            };
                            let new_offset = offset + sub_count * i as u64;
                            changes.append(&mut add_all(store, &sub_ctx, &sub_node, new_offset)?);
                        }
                        (Some(prev_link), Some(curr_link)) => {
                            let prev_cid = match prev_link {
                                node::Link::Cid { cid, .. } => cid,
                                _ => {
                                    anyhow::bail!("Unchanged link expected")
                                }
                            };
                            let curr_cid = match curr_link {
                                node::Link::Cid { cid, .. } => cid,
                                _ => {
                                    anyhow::bail!("Unchanged link expected")
                                }
                            };

                            if prev_cid == curr_cid {
                                continue;
                            }

                            let prev_sub_ctx = NodeContext {
                                bit_width: prev_ctx.bit_width,
                                height: prev_ctx.height - 1,
                            };
                            let prev_sub_node = store
                                .get_cbor::<CollapsedNode<V>>(prev_cid)?
                                .context("Failed to get collapsed node from block store")?
                                .expand(prev_sub_ctx.bit_width)?;
                            let curr_sub_ctx = NodeContext {
                                bit_width: curr_ctx.bit_width,
                                height: curr_ctx.height - 1,
                            };
                            let curr_sub_node = store
                                .get_cbor::<CollapsedNode<V>>(curr_cid)?
                                .context("Failed to get collapsed node from block store")?
                                .expand(curr_sub_ctx.bit_width)?;
                            let new_offset = offset + sub_count * i as u64;
                            changes.append(&mut diff_node(
                                store,
                                &prev_sub_ctx,
                                &prev_sub_node,
                                &curr_sub_ctx,
                                &curr_sub_node,
                                new_offset,
                            )?);
                        }
                    };
                }

                Ok(changes)
            }
            _ => {
                anyhow::bail!("Nodes has no links");
            }
        }
    }
}
