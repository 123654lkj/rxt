// rxt count / wc — 行/词/字符/字节统计
use std::io::{self, Read};

pub fn run(
    input: Option<&str>,
    lines_only: bool,
    words_only: bool,
    chars_only: bool,
    bytes_only: bool,
    max_line: bool,
) -> anyhow::Result<()> {
    let content: Vec<u8>;
    let text: &str;

    if let Some(path) = input {
        content = std::fs::read(path)?;
        text = std::str::from_utf8(&content)
            .map_err(|_| anyhow::anyhow!("file is not valid UTF-8"))?;
    } else {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        content = buf;
        text = std::str::from_utf8(&content)
            .map_err(|_| anyhow::anyhow!("stdin is not valid UTF-8"))?;
    }

    let all = !lines_only && !words_only && !chars_only && !bytes_only && !max_line;

    if all || lines_only {
        let line_count = text.lines().count();
        if lines_only {
            println!("{}", line_count);
        } else {
            print!("{:>8}", line_count);
        }
    }

    if all || words_only {
        let word_count = text.split_whitespace().count();
        if words_only {
            println!("{}", word_count);
        } else {
            print!("{:>8}", word_count);
        }
    }

    if all || chars_only {
        let char_count = text.chars().count();
        if chars_only {
            println!("{}", char_count);
        } else {
            print!("{:>8}", char_count);
        }
    }

    if all || bytes_only {
        if bytes_only {
            println!("{}", content.len());
        } else {
            print!("{:>8}", content.len());
        }
    }

    if max_line {
        let max = text.lines().map(|l| l.len()).max().unwrap_or(0);
        println!("{}", max);
    }

    if all {
        let label = input.unwrap_or("");
        if !label.is_empty() {
            println!(" {}", label);
        } else {
            println!();
        }
    }

    Ok(())
}
