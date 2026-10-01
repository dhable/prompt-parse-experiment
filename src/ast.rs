use std::fmt::Display;
use std::fmt::Write;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, PartialEq)]
pub struct ToolNamespace(pub Arc<str>);
impl From<&str> for ToolNamespace {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl Display for ToolNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub struct ToolName(pub Arc<str>);
impl From<&str> for ToolName {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
impl Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, PartialEq)]
pub struct SkillName(pub Arc<str>);
impl From<&str> for SkillName {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
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

impl PromptElement {
    pub fn new_content(content: impl Into<Arc<str>>) -> Self {
        let content = content.into();
        Self::PromptContent(content.into())
    }

    pub fn new_tool(ns: impl Into<ToolNamespace>, name: impl Into<ToolName>) -> Self {
        Self::ToolRef {
            ns: ns.into(),
            name: name.into(),
        }
    }

    pub fn new_skill(name: impl Into<SkillName>) -> Self {
        Self::SkillRef { name: name.into() }
    }

    pub fn new_include(path: PathBuf) -> Self {
        Self::IncludeRef { path }
    }
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
