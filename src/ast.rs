use std::fmt::Display;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, PartialEq)]
pub struct ToolNamespace(pub Arc<str>);
impl Display for ToolNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub struct ToolName(pub Arc<str>);
impl Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub struct SkillName(pub Arc<str>);
impl Display for SkillName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub enum PromptElement {
    PromptContent(Arc<str>),
    ToolRef { ns: ToolNamespace, name: ToolName },
    SkillRef { name: SkillName },
    IncludeRef { path: PathBuf },
}
impl Display for PromptElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PromptContent(c) => c.fmt(f),
            Self::ToolRef { ns, name } => {
                f.write_char('`')?;
                ns.fmt(f)?;
                f.write_str("__")?;
                name.fmt(f)?;
                f.write_char('`')
            }
            Self::SkillRef { name } => name.fmt(f),
            Self::IncludeRef { path } => {
                let content = fs::read_to_string(&path).expect("should be able to read the file");
                f.write_str(&content)
            }
        }
    }
}

/// A comment that vanishes mid-sentence leaves two `PromptContent` runs where the
/// prompt had one. Fold them back together so the element list mirrors the prose.
/// `flatten` is what discards the comments' `None`s.
pub fn coalesce(parts: Vec<Option<PromptElement>>) -> Vec<PromptElement> {
    let mut out: Vec<PromptElement> = Vec::new();
    for part in parts.into_iter().flatten() {
        if let PromptElement::PromptContent(next) = &part {
            // Borrow ends at the `continue`, so the `push` below stays legal.
            if let Some(PromptElement::PromptContent(prev)) = out.last_mut() {
                *prev = format!("{prev}{next}").into();
                continue;
            }
        }
        out.push(part);
    }
    out
}
