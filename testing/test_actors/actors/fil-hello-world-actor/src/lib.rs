// Copyright 2021-2023 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT
/// Placeholder invoke for testing
#[unsafe(no_mangle)]
#[cfg(target_arch = "wasm32")]
pub fn invoke(_: u32) -> u32 {
    use fvm_sdk as sdk;

    // Conduct method dispatch. Handle input parameters and return data.
    sdk::vm::abort(
        fvm_shared::error::ExitCode::FIRST_USER_EXIT_CODE,
        Some("sample abort"),
    )
}
