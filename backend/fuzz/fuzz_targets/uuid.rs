#![no_main]

use libfuzzer_sys::fuzz_target;
use warrior_fuzz::{run, Target};

fuzz_target!(|data: &[u8]| run(Target::Uuid, data));
