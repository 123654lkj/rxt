/// UUID 生成器
pub fn run(count: usize) -> anyhow::Result<()> {
    for _ in 0..count {
        println!("{}", uuid::Uuid::new_v4());
    }
    Ok(())
}
