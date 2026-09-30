//-------------------------------------------------------------------------------------------
// The verbose grammar parser and example prompt
//-------------------------------------------------------------------------------------------
use crate::ast::{PromptElement, SkillName, ToolName, ToolNamespace, coalesce};

peg::parser! {
    pub grammar prompt_parser() for str {
        // File system path definition. The prompt file path syntax does not support relative
        // parent directory or absolute path syntax to sandbox what the prompt can reference.
        rule inner_name()      = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-']+
        rule extension()       = ['a'..='z' | 'A'..='Z' | '0'..='9']+
        rule file_name()       = inner_name() ("." extension())?
        rule dir_name()        = inner_name() "/"
        rule path() -> &'input str
                               = path:$(dir_name()+ file_name()? / file_name()) { path }

        // Embedded tool name patterns
        rule literal_char()    = ['a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '/' | '.']
        rule wildcard()        = "*" / "?"
        rule class_match()     = "[" "!"? literal_char()+ "]"
        rule alt_branch()      = (literal_char() / wildcard() / class_match())*
        rule alt_match()       = "{" alt_branch() ++ "," "}"
        rule glob_match()      = wildcard() / class_match() / alt_match()
        rule glob_element()    = literal_char()* glob_match() (literal_char() / glob_match())*
        rule literal_element() = literal_char()*<1,64>
        rule element()         = glob_element() / literal_element()
        rule ns_pattern() -> ToolNamespace
                               = ns:$element() ":" { ToolNamespace(ns.into()) }
        rule name_pattern() -> ToolName
                               = n:$element() { ToolName(n.into()) }
        rule tool_fqn() -> PromptElement
                               = ns:ns_pattern() name:name_pattern() { PromptElement::ToolRef { ns, name } }

        rule sigil() = "@"
        rule tool_ref() -> PromptElement = sigil() "Tool(" t:tool_fqn() ")" { t }
        rule include_ref() -> PromptElement = sigil() "Include(" p:path() ")" { PromptElement::IncludeRef { path: p.into() }}
        rule skill_ref() -> PromptElement = sigil() "Skill(" p:path() ")" { PromptElement::SkillRef { name: SkillName(p.into()) }}
        rule prompt_commands() -> PromptElement = tool_ref() / skill_ref() / include_ref()

        // Comments do not nest, so the body stops at the first closer. comment_open() guards
        // content while comment() consumes it; the gap between the two is deliberate, and is
        // what makes an unterminated comment an error instead of prose. Keep them in sync.
        rule comment_open()    = "<!--"
        rule comment()         = comment_open() (!"-->" [_])* "-->"

        // prompt literal content
        rule code_block_fence()
                               = "```" (!"```" [_])* "```"
        rule content_char()    = code_block_fence()
                               / (!prompt_commands() !comment_open() [_])
        rule content_literal() -> PromptElement
                               = c:$(content_char()+) { PromptElement::PromptContent(c.into()) }

        // A comment parses to None: consumed from the stream, never emitted. Everything
        // else carries a value through, and coalesce() drops the Nones.
        rule prompt_part() -> Option<PromptElement>
                               = comment() { None }
                               / e:(prompt_commands() / content_literal()) { Some(e) }

        // refs must come first: content_literal() would otherwise always win.
        pub rule prompts() -> Vec<PromptElement>
                               = parts:prompt_part()* { coalesce(parts) }
    }
}

pub const EXAMPLE_PROMPT: &'static str = r#"
You are a site reliability engineering agent. The goal of our SRE team is to ensure quick
resolution to system issues with minimal downtime. Our stack includes:

<!--
    the list of services are maintained by a cron script. instead of splicing them into each
    prompt, we can just use the include syntax to bring them into the prompt script.
-->
@Include(sample/snippets/service_def.md)

When inspecting the business services, you should be able to use the @Tool(mezmo_internal:describe) tool in order to
find out service specific details. Always include a kubernetes namespace on all @Tool(k8s:*) tools. Fall back to
the @Tool(`find_service`) tool if @Tool(mezmo_internal:describe) returns no results.

```
<!-- this isn't a comment since it's in a code block -->
<h1>Code Block</h1>
```

`not_a_tool:just_documentation` <!-- another comment -->

Generate a report using @Skill(skills/audit-report.md) of deployed node.js services that have a trace calling the
`internal_mezmo_auth` function in the `mezmo_auth` package. You might also need to look at deployed infra for sidecar
applications that use the `mezmo_auth` package. Group the report into sections by deployment environment. Send
the report to `ops@mezmo.com`.
"#;
