fn main() {
    horizon_ios_platform::initialize().expect("Cannot initialize the iOS sandbox");
    horizon_client::entry::main();
}
