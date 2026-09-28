---
name: reduce-verbose
description: Instructions on how to generate summary descriptions without a lot of AI verbosity.
---

Write the summary, then delete everything the reader can infer. Length is not the
problem; low information density is. A short summary that names versions, counts, and
paths beats a long one that calls things "robust."

## Lead with the finding

The first sentence carries the conclusion. No preamble, no restating the request, no
announcing what you are about to do. If the reader stops after one sentence, they should
still have the answer.

## Cut on sight

| Pattern | Instead |
|---|---|
| "Let me analyze...", "Here's a breakdown:" | Open with the finding |
| "In summary,", "Overall," | Stop when you are done |
| "It's worth noting that X" | "X" |
| "may potentially be somewhat" | One hedge, or none |
| robust, seamless, powerful, comprehensive, key, crucial | A specific fact, or nothing |
| leverage, utilize, delve into | use, read |
| "performs a validation of" | "validates" |
| "There are three things that need attention" | Name the three |
| "clear, concise, and readable" | One adjective |
| A third list item added for symmetry | Two items |
| A header over two sentences | Just the sentences |

## Keep the specifics

Numbers, versions, service names, paths, error text, dates. Cutting these is not brevity,
it is vagueness, and it costs the reader a round trip to ask for what you dropped.

## Say what you do not know

"No data for staging" is short and useful. Hedged prose that implies coverage you do not
have is long and misleading. Name the gap instead of writing around it.

## Revision pass

Delete each sentence in turn. If the summary still says the same thing, leave it deleted.

## Example

Before:

> I've completed a comprehensive analysis of the authentication service. It's worth
> noting that this service is a crucial component of the overall infrastructure. The
> analysis revealed several key issues that may potentially need to be addressed. First
> and foremost, the service appears to be running an older version. Additionally, it
> seems that resource limits have not been configured. In summary, the authentication
> service would benefit from some attention in these areas.

After:

> `mezmo-auth` runs v2.1.4; v2.4.0 is current. The deployment sets no CPU or memory
> limits. Both apply to prod.

The second version is a third the length and carries more: the service name, both
version numbers, and the environment. The first version never gave any of them.
