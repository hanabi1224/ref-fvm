// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
use fvm::executor::{ApplyKind, Executor};
use fvm_integration_tests::dummy::DummyExterns;
use fvm_integration_tests::tester::{Account, Tester};
use fvm_ipld_blockstore::MemoryBlockstore;
use fvm_ipld_encoding::RawBytes;
use fvm_ipld_encoding::tuple::*;
use fvm_shared::address::Address;
use fvm_shared::econ::TokenAmount;
use fvm_shared::error::ExitCode;
use fvm_shared::message::Message;
use fvm_shared::state::StateTreeVersion;
use fvm_shared::version::NetworkVersion;
use fvm_test_actors::wasm_bin::INTEGER_OVERFLOW_ACTOR_BINARY;
use num_traits::Zero;

mod bundles;
use bundles::*;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug, Default)]
pub struct State {
    pub value: i64,
}

// Utility function to instantiation integration tester
fn instantiate_tester() -> (Account, Tester<MemoryBlockstore, DummyExterns>, Address) {
    // Instantiate tester
    let mut tester = new_tester(
        NetworkVersion::V21,
        StateTreeVersion::V5,
        MemoryBlockstore::default(),
    )
    .unwrap();

    let sender: [Account; 1] = tester.create_accounts().unwrap();

    // Set actor state
    let actor_state = State::default();
    let state_cid = tester.set_state(&actor_state).unwrap();

    // Set actor
    let actor_address = Address::new_id(10000);

    // Get wasm bin
    let wasm_bin = INTEGER_OVERFLOW_ACTOR_BINARY;

    tester
        .set_actor_from_bin(wasm_bin, state_cid, actor_address, TokenAmount::zero())
        .unwrap();

    (sender[0], tester, actor_address)
}

#[test]
fn integer_overflow() {
    // Instantiate tester
    let (sender, mut tester, actor_address) = instantiate_tester();

    // Instantiate machine
    tester.instantiate_machine(DummyExterns).unwrap();

    // X is the target value.
    let x: i64 = 10000000000;

    {
        // Params setup
        let params = RawBytes::serialize(x).unwrap();

        // Send message to set
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 1,
            params,
            ..Message::default()
        };

        // Set inner state value
        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();

        assert_eq!(
            ExitCode::OK,
            res.msg_receipt.exit_code,
            "{}",
            res.failure_info.unwrap()
        );

        // Read inner state value
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 3,
            sequence: 1,
            ..Message::default()
        };

        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();
        assert!(res.msg_receipt.exit_code.is_success());

        let current_state_value: i64 = res.msg_receipt.return_data.deserialize().unwrap();

        assert_eq!(current_state_value, x);
    }

    {
        // Overflow inner state integer with checked overflows.
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 4,
            sequence: 2,
            ..Message::default()
        };

        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();
        assert_eq!(ExitCode::SYS_ILLEGAL_INSTRUCTION, res.msg_receipt.exit_code);

        // Read inner state value
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 3,
            sequence: 3,
            ..Message::default()
        };

        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();
        assert!(res.msg_receipt.exit_code.is_success());

        let current_state_value: i64 = res.msg_receipt.return_data.deserialize().unwrap();
        assert_eq!(current_state_value, x);
    }

    {
        // Overflow inner state integer with wrapping.
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 2,
            sequence: 4,
            ..Message::default()
        };

        // Set inner state value
        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();
        assert_eq!(
            ExitCode::OK,
            res.msg_receipt.exit_code,
            "{}",
            res.failure_info.unwrap()
        );

        // Read inner state value
        let message = Message {
            from: sender.1,
            to: actor_address,
            gas_limit: 1000000000,
            method_num: 3,
            sequence: 5,
            ..Message::default()
        };

        let res = tester
            .executor
            .as_mut()
            .unwrap()
            .execute_message(message, ApplyKind::Explicit, 100)
            .unwrap();
        assert!(res.msg_receipt.exit_code.is_success());

        let current_state_value: i64 = res.msg_receipt.return_data.deserialize().unwrap();

        // Check overflow
        let overflow_value: i64 = -5340232216128654848;

        assert_eq!(current_state_value, overflow_value);
    }
}
