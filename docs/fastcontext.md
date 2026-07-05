Карточку HF у меня прямо не открыло, так что я не буду врать, будто видел tokenizer/config/quantization. Работаю по имени модели: PhysShell/FastContext-1.0-4B-SFT, то есть предполагаю обычную SFT text-generation модель на Hugging Face. Если там GGUF, встраивание одно. Если только safetensors, другое. Да, человечество опять изобрело “одна модель, пять несовместимых способов запустить”, восхитительно.

Куда её прикручивать в 007

Не как замену Claude/Codex для o7 run.
Сейчас 007 устроен как harness: git worktree → запуск агента → gate → harvest run record. Это прямо описано в README: isolate, run, gate, harvest, плюс сохранение task.md, meta.json, agent.stdout, diff.patch и gate-логов.

В коде o7 run сейчас вызывает agent::run(engine, wt, task, &a.model, a.max_turns), то есть ожидает агента, который реально может работать в worktree.  А src/agent.rs пока знает только Claude и Codex, причём codex для full-auto run ещё прямо помечен как не wired.

FastContext лучше вешать как локальный “context/judge/router” слой:

1. o7 context
Быстро собирает релевантный контекст по repo/task, сжимает его FastContext-ом в context.md.


2. o7 run --context ./context.md --engine claude|codex
Claude/Codex остаются исполнителями, а FastContext даёт им подготовленный контекст.


3. o7 judge --provider fastcontext
Локальный дешёвый pre-judge для analyzer findings. Claude/Codex оставить для сложных/uncertain кейсов.


4. Позже: o7 route
FastContext классифицирует задачу: “простая правка”, “нужен Claude”, “нужен Codex”, “нужен human”. Вот тут оно реально полезно, а не просто игрушечная LLM-надстройка с флагом --ai, как любят делать люди, когда им мало багов.



Архитектура, которую я бы делал

007
├── src/
│   ├── agent.rs          # Claude/Codex full-auto executor
│   ├── judge.rs          # FP triage
│   ├── context.rs        # НОВОЕ: repo/task -> context brief
│   ├── local_llm.rs      # НОВОЕ: OpenAI-compatible HTTP client
│   └── main.rs           # CLI: context / judge provider / run --context
└── examples/
    └── context.own.net.toml

Главная идея: не тащить Hugging Face runtime внутрь Rust-бинаря. Это будет больше зависимостей, чем смысла. 007 сейчас специально маленький Rust CLI: clap, serde, toml, anyhow, hash libs, без native/sys-зависимостей.  Flake тоже говорит: pure Rust, no native/sys deps.

Поэтому модель должна жить снаружи как локальный HTTP endpoint:

# примерная форма, зависит от формата модели
export O7_FASTCONTEXT_URL=http://127.0.0.1:8000/v1/chat/completions
export O7_FASTCONTEXT_MODEL=PhysShell/FastContext-1.0-4B-SFT

А 007 вызывает её через OpenAI-compatible /v1/chat/completions.

Как запускать модель

Вариант A: если есть GGUF

Тогда проще всего llama.cpp/llama-server или Ollama. Это самый практичный вариант для локального 4B, особенно если нужна CPU/quantized история. llama.cpp поддерживает GGUF и OpenAI-compatible endpoints вроде /v1/chat/completions; GGUF хранит веса и metadata модели в одном бинарном формате и часто используется для quantized моделей. 

Схема:

llama-server \
  -m ./FastContext-1.0-4B-SFT.Q4_K_M.gguf \
  --host 127.0.0.1 \
  --port 8000 \
  -c 32768

Потом:

o7 context \
  --repo ../Own.NET \
  --task ./task.md \
  --provider fastcontext \
  --model PhysShell/FastContext-1.0-4B-SFT \
  --out /tmp/o7-context.md

Вариант B: если только HF safetensors

Тогда лучше vLLM или TGI/Transformers server. vLLM хорош, если есть GPU и нужны throughput/batching; его смысл как раз в serving LLM с эффективной работой KV-cache через PagedAttention.  

Примерная форма:

vllm serve PhysShell/FastContext-1.0-4B-SFT \
  --host 127.0.0.1 \
  --port 8000 \
  --served-model-name fastcontext

Тогда в 007:

export O7_FASTCONTEXT_MODEL=fastcontext
export O7_FASTCONTEXT_URL=http://127.0.0.1:8000/v1/chat/completions

Минимальный patch design

1. Добавить локальный LLM client

src/local_llm.rs:

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct LocalLlmConfig {
    pub url: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    temperature: f32,
    max_tokens: u32,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Deserialize)]
struct ResponseMessage {
    content: String,
}

pub fn chat(cfg: &LocalLlmConfig, system: &str, user: &str) -> Result<String> {
    let req = ChatRequest {
        model: &cfg.model,
        temperature: cfg.temperature,
        max_tokens: cfg.max_tokens,
        messages: vec![
            Message {
                role: "system",
                content: system,
            },
            Message {
                role: "user",
                content: user,
            },
        ],
    };

    let resp: ChatResponse = ureq::post(&cfg.url)
        .set("content-type", "application/json")
        .send_json(ureq::json!(req))
        .with_context(|| format!("calling local LLM at {}", cfg.url))?
        .into_json()
        .context("parsing local LLM response")?;

    resp.choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .context("local LLM returned no choices")
}

В Cargo.toml:

ureq = { version = "2", features = ["json"] }

Да, ureq, не сразу async tower-of-babel. Сейчас 007 блокирующий subprocess harness, и тащить tokio ради одного локального HTTP call будет “Your function does more jobs than a single parent on three shifts!” в архитектурной форме.

2. Добавить Engine::FastContext, но не в full-auto executor

В src/agent.rs:

pub enum Engine {
    Claude,
    Codex,
    FastContext,
}

label():

Engine::FastContext => "fastcontext",

FromStr:

"fastcontext" => Ok(Engine::FastContext),

Но в agent::run:

Engine::FastContext => anyhow::bail!(
    "fastcontext is not a full-auto editing agent; use it through `o7 context` or `o7 judge --provider fastcontext`"
),

Это важно. Иначе вы получите модель, которая умеет писать текст, но не умеет безопасно редактировать repo, и будете потом строить tool loop, file patch protocol, sandbox, retries, JSON repair, rollback. Just a quick hack?! That's technical debt dressed up as urgency!

3. Добавить o7 context

CLI:

enum Cmd {
    Run(RunArgs),
    Judge(judge::JudgeArgs),
    Context(context::ContextArgs),
}

ContextArgs:

#[derive(clap::Args)]
pub struct ContextArgs {
    #[arg(long)]
    pub repo: PathBuf,

    #[arg(long)]
    pub task: PathBuf,

    #[arg(long)]
    pub out: PathBuf,

    #[arg(long, default_value = "fastcontext")]
    pub model: String,

    #[arg(long, default_value = "http://127.0.0.1:8000/v1/chat/completions")]
    pub url: String,

    #[arg(long, default_value_t = 16)]
    pub max_files: usize,
}

Что делает context::run:

1. read task.md
2. collect candidate files:
   - git diff --name-only base..HEAD
   - ripgrep по ключевым словам из task
   - .007/context.toml include/exclude
3. cap by file count and byte budget
4. build prompt:
   - task
   - file list
   - selected snippets / whole small files
5. call FastContext
6. write context.md

Формат context.md:

# Context Brief

## Task interpretation
...

## Relevant files
- `src/...`: why relevant
- `tests/...`: why relevant

## Risks
...

## Suggested execution plan for Claude/Codex
...

## Do not touch
...

Потом:

cat /tmp/o7-context.md ./task.md > /tmp/task.with-context.md

o7 run \
  --repo ../Own.NET \
  --base HEAD \
  --task /tmp/task.with-context.md \
  --engine claude \
  --model opus

Позже это можно завернуть в o7 run --context-provider fastcontext, но сначала отдельная команда. Меньше магии, меньше “а почему агент решил не то”, меньше багов, меньше шаманства с логами.

4. В judge добавить --provider fastcontext

judge уже архитектурно близко: он читает findings, группирует по файлам, собирает prompt и вызывает backend. Сейчас backend — Claude/Codex subprocess.  Вызов агента централизован через call_agent(provider, ...), где сейчас match на Claude/Codex.

Туда FastContext ложится идеально:

fn call_agent(
    provider: Engine,
    cwd: &Path,
    prompt: &str,
    model: &str,
) -> Result<(String, Option<String>, Option<f64>)> {
    match provider {
        Engine::Claude => call_claude(cwd, prompt, model),
        Engine::Codex => call_codex(cwd, prompt, model),
        Engine::FastContext => call_fastcontext(prompt, model),
    }
}

call_fastcontext:

fn call_fastcontext(
    prompt: &str,
    model: &str,
) -> Result<(String, Option<String>, Option<f64>)> {
    let url = std::env::var("O7_FASTCONTEXT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8000/v1/chat/completions".to_string());

    let cfg = crate::local_llm::LocalLlmConfig {
        url,
        model: model.to_string(),
        temperature: 0.0,
        max_tokens: 2048,
    };

    let system = "Return only the requested JSON. No prose. No markdown fences.";
    let text = crate::local_llm::chat(&cfg, system, prompt)?;

    Ok((text, None, None))
}

И в provider resolver:

"fastcontext" => Ok(Engine::FastContext),

auto я бы не трогал. Пусть auto остаётся Claude/Codex routing. FastContext надо включать явно:

o7 judge \
  --repo ../Own.NET \
  --findings findings.json \
  --rubric judge/rubric.md \
  --provider fastcontext \
  --model fastcontext \
  --out fp-verdicts.local.json

Где это особенно зайдёт для Own.NET / OwnAudit

Для 007 самое полезное место FastContext-а:

analyzer findings
      ↓
FastContext cheap local triage
      ↓
real / false_positive / uncertain
      ↓
Claude/Codex only for uncertain or high-risk
      ↓
final fp-verdicts.json

Сейчас performance notes говорят, что 007 subprocess/LLM-bound, а главный рычаг — не микротюнинг Rust, а параллелизация judge calls.   FastContext даёт второй рычаг: часть judge calls вообще не отправлять дорогим моделям.

То есть pipeline:

# 1. дешёвый локальный проход
o7 judge \
  --provider fastcontext \
  --model fastcontext \
  --repo ../Own.NET \
  --findings findings.json \
  --rubric judge/rubric.md \
  --out fp-verdicts.fastcontext.json

# 2. дорогой проход только по uncertain/high-risk
o7 judge \
  --provider claude \
  --model opus \
  --repo ../Own.NET \
  --findings findings.uncertain.json \
  --rubric judge/rubric.md \
  --out fp-verdicts.claude.json

Потом merge overlay. Вот это уже инженерия, а не “прикрутил модель, теперь у нас AI”.

Security footgun, потому что он тут жирный

FastContext локальный, но это не значит “можно пихать всё подряд”. В judge у вас уже есть хорошая привычка: path confinement, canonicalize, проверка что файл остаётся внутри repo root.  Claude judge path тоже специально отключает tools и ambient MCP, чтобы source-in-prompt не стал exfiltration цирком.

Для o7 context надо сразу сделать .007/context.toml:

max_files = 20
max_file_bytes = 24000
max_total_bytes = 180000

include = [
  "src/**/*.cs",
  "src/**/*.rs",
  "tests/**/*.cs",
  "*.sln",
  "*.csproj"
]

exclude = [
  "**/.git/**",
  "**/bin/**",
  "**/obj/**",
  "**/packages/**",
  "**/*.pfx",
  "**/*.snk",
  "**/*.key",
  "**/*.secret.*",
  "**/appsettings.Production.json"
]

Иначе WHERE'S THE ERROR HANDLING?! Вы сами сделаете prompt-leak комбайн, который заботливо складывает секреты в runs/.../raw.txt.

Итоговая рекомендация

MVP для 007:

1. Запустить FastContext отдельно через OpenAI-compatible local server.


2. Добавить src/local_llm.rs.


3. Добавить o7 context.


4. Добавить judge --provider fastcontext.


5. Не разрешать run --engine fastcontext как full-auto agent.


6. Сохранять context.md в run record рядом с task.md, meta.json, agent.stdout.


7. Добавить .007/context.toml с include/exclude/size caps.


8. Позже сделать cascade judge: FastContext first, Claude/Codex only for uncertain.



Самый чистый mental model:

FastContext = быстрый локальный мозг для сжатия, отбора, pre-triage
Claude/Codex = дорогие исполнители и финальные судьи
007 = оркестратор, sandbox, record keeper, gatekeeper

Так оно встраивается в текущую форму 007 без архитектурного самострела.