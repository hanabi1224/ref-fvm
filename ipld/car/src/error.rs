// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use thiserror::Error;

/// Car utility error
#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to parse CAR file: {0}")]
    ParsingError(String),
    #[error("Invalid CAR file: {0}")]
    InvalidFile(String),
    #[error("Io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Cbor encoding error: {0}")]
    Cbor(#[from] fvm_ipld_encoding::Error),
    #[error("CAR error: {0}")]
    Other(String),
}

impl From<cid::Error> for Error {
    fn from(err: cid::Error) -> Error {
        Error::Other(err.to_string())
    }
}

impl From<multihash_derive::UnsupportedCode> for Error {
    fn from(err: multihash_derive::UnsupportedCode) -> Error {
        Error::ParsingError(err.to_string())
    }
}
