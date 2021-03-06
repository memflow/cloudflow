fn main() {
    tonic_build::compile_protos("../memflow_rpc.proto").unwrap();
}
