use vergen::{ConstantsFlags, generate_cargo_keys};

fn main() {
    let mut flags = ConstantsFlags::all();
    //Only one of SEMVER_FROM_CARGO_PKG and SEMVER can be set
    //by unsetting this flag, version will be taken from git tags
    flags.remove(ConstantsFlags::SEMVER_FROM_CARGO_PKG);

    generate_cargo_keys(flags).unwrap();
}