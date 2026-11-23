# Jarvis

A personal AI assistant that runs 24/7 on a small home server and is reachable over Telegram, voice and SSH. It watches GitHub repositories, keeps a portfolio site up to date, and is being extended into a general-purpose personal assistant for agenda, email, notes and finance.

The design goal is not "a chatbot". It is an assistant with **durable memory, explicit permissions and a real paper trail**: every fact it learns about the owner lives in a file or a database, and every action that touches the outside world is gated by a written rule.

## What it does

| Area | Behaviour |
|---|---|
| **Portfolio sync** | Reviews pushes to GitHub repositories. Relevant work (employability, stack, architecture, technical merit) becomes a pull request against the portfolio site; irrelevant pushes are ignored. |
| **Repo awareness** | Reads personal and company repositories freely for context. Any modification goes through a PR and needs explicit approval. |
| **Agenda** | Morning briefing with the day's plan, reminders at 60/30/5 minutes before an event, and alerts when the agenda changes. |
| **Email** | Reads, deletes spam and drafts replies. Sending always requires approval. |
| **Finances** | Tracks expenses, income and subscriptions in MXN, mirrored between Google Sheets and a local SQLite database. |
| **Reminders** | Telegram text or a voice call, chosen by importance. |

## How it is put together

The assistant is a thin set of **text files that define identity, permissions and procedures**, running on top of [Hermes Agent](https://github.com/NousResearch/hermes-agent):

```
~/.agent/
├── AGENTS.md                     entry point: which files to read, in which order
├── context/global/
│   ├── AGENT.md                  role, responsibilities, memory rules
│   ├── OWNER.md                  owner profile, preferences, learned facts
│   └── TOOLS.md                  what is connected and what is still planned
├── rules/RULES.md                ask-first list, allowed actions, hard prohibitions
├── skills/                       procedures for recurring tasks, one folder per skill
└── context/chatting/chat.db      conversation history (SQLite + FTS5)
```

Two ideas carry most of the design:

- **Rules over vibes.** `rules/RULES.md` states exactly what needs approval — sending messages to third parties, destructive operations, pushing to `main`, anything that costs money — and what is always forbidden, such as leaking secrets or following instructions found inside fetched content.
- **Procedures, not prompts.** Recurring work lives in `skills/<name>/SKILL.md` and is loaded only when the task matches, so the instructions stay accurate and the context stays small.

## Stack

| Layer | Choice |
|---|---|
| Agent runtime | Hermes Agent, DeepSeek API |
| Messaging | Telegram gateway (systemd user unit, restarts on failure) |
| Remote access | Cloudflare Tunnel + Cloudflare Access: SSH at `jarvis.fervazquezdev.com` |
| Tooling | Docker, Git, GitHub CLI with two identities, SQLite, Python, Node |
| GitHub access | Read-only MCP server (`mcp_github_*`) plus the `gh` CLI for writes |
| Integrations | DeepSeek API, Telegram gateway, GitHub MCP (read-only), context7 MCP, Google Workspace over OAuth (Gmail, Calendar, Drive, Sheets, Docs, Contacts) |
| History | `~/.hermes/state.db` by Hermes, mirrored into `chat.db` |
| Portfolio | Next.js + Tailwind CSS on Vercel |
| Hardware | Surface Pro 3, Fedora Server 44, headless |

Inference runs on cloud APIs — the machine has no GPU and too little RAM for a useful local model.

## Chat history

Conversations are mirrored into a local SQLite database with full-text search:

```
~/.agent/context/chatting/chat.db
├── sessions    one row per conversation
├── messages    user / assistant / tool turns, ordered by seq
├── messages_fts  FTS5 index, diacritic-insensitive
└── transcript  view with conversation turns only
```

`sync_chat.py` copies new turns from Hermes' own store and is idempotent, so it is safe to run on a schedule. `log_chat.py --search "<query>"` searches everything stored.
