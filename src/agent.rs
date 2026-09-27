//! In-app ACP client and native design-tool host.
pub mod attachments;
pub mod bridge;
pub mod config;
pub mod discovery;
pub mod runtime;
pub mod tools;
pub mod workspace;
pub use workspace::Workspace;
#[cfg(test)]
mod tests;

pub const SKILL: &str = include_str!("../skills/omadesign-create/SKILL.md");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Purpose {
    Learn,
    #[default]
    Create,
}

pub fn cli(args: &[String]) -> Option<Result<(), String>> {
    if args.iter().any(|a| a == "--agent-mcp") {
        return Some(bridge::stdio());
    }
    if args.iter().any(|a| a == "--agent-skill") {
        print!("{SKILL}");
        return Some(Ok(()));
    }
    let index = args.iter().position(|a| a == "--agent-docs")?;
    Some(
        match args.get(index + 1).map(String::as_str).unwrap_or("index") {
            "manual" => {
                print!("{}", include_str!("../docs/MANUAL.md"));
                Ok(())
            }
            "layout" => {
                print!("{}", include_str!("../docs/layout.md"));
                Ok(())
            }
            "plugins" => {
                print!("{}", include_str!("../docs/plugins.md"));
                Ok(())
            }
            "formats" => {
                print!("{}", include_str!("../docs/format-support.md"));
                Ok(())
            }
            "index" => {
                print!("{}", include_str!("../site/public/llms.txt"));
                Ok(())
            }
            _ => Err("Choose --agent-docs index, manual, layout, formats, or plugins".into()),
        },
    )
}
