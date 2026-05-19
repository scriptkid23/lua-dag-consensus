//! LUA-DAG validator binary entry point.

fn main() -> anyhow::Result<()> {
    node::runtime::run()
}
