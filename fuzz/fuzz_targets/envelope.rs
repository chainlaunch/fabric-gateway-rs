// SPDX-License-Identifier: Apache-2.0
#![no_main]
use libfuzzer_sys::fuzz_target;

// Property: the parse_envelope decoder never panics on arbitrary input.
fuzz_target!(|data: &[u8]| {
    fabric_gateway::fuzz_api::parse_envelope(data);
});
