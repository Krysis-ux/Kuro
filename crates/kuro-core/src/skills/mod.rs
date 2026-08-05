//! Skills: expertise you switch on.
//!
//! A skill is a short, specific instruction block appended to the system prompt.
//! That is all. There is no execution, no sandbox, no plugin runtime — and that
//! restraint is the point, because it is what makes a skill safe to install with
//! one click and impossible to break the application with.
//!
//! The reason this is worth having at all is that prompt guidance is the highest
//! leverage available on a small local model. A 4B model asked for Rust will
//! cheerfully write code that does not compile; the same model told "every snippet
//! must include its `use` statements, prefer `?` over `unwrap`, and say which
//! edition you assume" produces materially better output. That is a real gain for
//! zero inference cost, and it is exactly the gain a hosted frontier model gets
//! from a system prompt somebody spent a month on.
//!
//! Skills are deliberately not auto-selected *per message*. Guessing which
//! expertise a given question needs is a classification problem that would be
//! wrong often enough to be annoying, and being wrong here means silently
//! changing how the model answers.
//!
//! A fresh install does start with the coding category switched on — see
//! [`default_slugs`]. That is a starting position, not a guess about any
//! particular message: it is visible in the store, costed in the same token
//! count as everything else, and overwritten the moment the user touches a
//! switch.

use serde::Serialize;

use crate::db::Db;
use crate::Result;

/// Settings key holding the slugs the user switched on.
pub const KEY_ENABLED: &str = "skills.enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Language,
    /// Working in a real codebase with tools: navigating it, changing it safely,
    /// and checking the change. These matter most on the Code page, where the
    /// model can actually act on what it reads.
    Coding,
    Practice,
    Design,
    Writing,
}

impl SkillCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Language => "Languages",
            Self::Coding => "Working in a codebase",
            Self::Practice => "Engineering practice",
            Self::Design => "Interface and design",
            Self::Writing => "Writing and reasoning",
        }
    }

    /// Every category, in the order the store shows them.
    pub const ALL: &'static [SkillCategory] = &[
        Self::Language,
        Self::Coding,
        Self::Practice,
        Self::Design,
        Self::Writing,
    ];
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub slug: &'static str,
    pub name: &'static str,
    /// One line, for the card.
    pub blurb: &'static str,
    pub category: SkillCategory,
    /// What gets appended to the system prompt. Written as imperatives, because
    /// that is what a small model follows most reliably.
    pub instructions: &'static str,
    /// Roughly how much context this costs, so the UI can warn when several are on.
    pub approx_tokens: usize,
    /// Always on inside a coding workspace, and not shown in the store.
    ///
    /// A short list, and each entry earns it by describing something that is not
    /// a preference. "Read a file before you edit it" is not a style anybody
    /// might reasonably switch off; it is the difference between an assistant
    /// that edits code and one that destroys it. Putting such a rule on a toggle
    /// implies there is a sensible reason to turn it off, and there is not — so
    /// they are hidden rather than merely defaulted on, because a default-on
    /// switch is a switch somebody eventually flips while tidying.
    pub essential: bool,
}

/// The catalogue.
///
/// Every entry is guidance a competent reviewer of that language would actually
/// give. Nothing here is filler: a skill that says "write good code" would cost
/// context and change nothing.
pub const SKILLS: &[Skill] = &[
    Skill {
        slug: "rust",
        name: "Rust",
        blurb: "Compiling code, real error handling, no stray unwraps.",
        category: SkillCategory::Language,
        approx_tokens: 130,
        essential: false,
        instructions: "\
When writing Rust:
- Include every `use` statement the snippet needs. A snippet that does not compile is not an answer.
- Return `Result<T, E>` and use `?`. Reserve `unwrap`/`expect` for tests and genuinely unreachable states, and say which it is.
- Borrow (`&str`, `&[T]`) in parameters; take ownership only when you store or consume the value.
- Never clone to silence the borrow checker without saying why the clone is correct.
- Say which edition you assume when it matters (2021 unless told otherwise).
- Prefer iterator chains for transformation, loops for control flow with early exits.
- For async, name the runtime (`tokio` unless told otherwise) and do not block inside an async fn.",
    },
    Skill {
        slug: "python",
        name: "Python",
        blurb: "Type hints, real error handling, stdlib before dependencies.",
        category: SkillCategory::Language,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When writing Python:
- Target 3.11+ unless told otherwise. Use `list[str]`, `dict[str, int]`, `X | None` — not `typing.List` or `Optional`.
- Add type hints to every function signature you write.
- Catch specific exceptions, never bare `except:`. Never swallow an exception silently.
- Use `pathlib` over `os.path`, f-strings over `%` and `.format`.
- Prefer the standard library. Name any third-party dependency explicitly and say why it is worth adding.
- Use a `if __name__ == \"__main__\":` guard for anything runnable.
- Never mutate a default argument; use `None` and build inside the function.",
    },
    Skill {
        slug: "typescript",
        name: "TypeScript",
        blurb: "No `any`, narrow unknown, correct async.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing TypeScript:
- Never use `any`. Use `unknown` for untrusted input and narrow it before use.
- Type every exported function's parameters and return value. Let local inference do the rest.
- Use `type` for unions and `interface` for object shapes that may be extended.
- Prefer string literal unions over `enum`.
- `await` every promise or handle it explicitly; never leave a floating promise.
- Update state immutably — spread rather than assign into an existing object.
- Say which runtime you assume (Node, browser, Deno) when it changes the answer.",
    },
    Skill {
        slug: "go",
        name: "Go",
        blurb: "Idiomatic error wrapping, no goroutine leaks.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing Go:
- Handle every error. Wrap with context: `fmt.Errorf(\"doing X: %w\", err)`.
- Never use `panic` for ordinary failure, and never ignore an error with `_` without saying why.
- Accept interfaces, return concrete types.
- Pass `context.Context` as the first parameter to anything that does I/O, and honour cancellation.
- Every goroutine you start must have a stated way to stop. Use `defer` for cleanup.
- Guard shared state with a mutex or a channel, and say which and why.
- Keep to the standard library unless a dependency is clearly justified.",
    },
    Skill {
        slug: "sql",
        name: "SQL",
        blurb: "Parameterised queries, indexes, no accidental table scans.",
        category: SkillCategory::Language,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When writing SQL:
- Always use bound parameters. Never concatenate a value into a query string, not even in an example.
- Name the dialect you are writing for (PostgreSQL unless told otherwise) — the syntax differs.
- Say which index a query relies on, and flag any query that would scan a whole table.
- Prefer explicit `JOIN ... ON` over `WHERE` joins, and never `SELECT *` in application code.
- Add `LIMIT` to anything that could return an unbounded result set.
- For a migration, give the rollback too, and flag any statement that locks a table.",
    },
    Skill {
        slug: "shell",
        name: "Shell",
        blurb: "Safe scripts that fail loudly instead of half-working.",
        category: SkillCategory::Language,
        approx_tokens: 90,
        essential: false,
        instructions: "\
When writing shell scripts:
- Start with `set -euo pipefail`. A script that continues after a failed step is a hazard.
- Quote every variable expansion: `\"$var\"`, `\"${array[@]}\"`.
- Use `$(...)` not backticks, and `[[ ]]` not `[ ]` in bash.
- Never parse `ls`. Use globs or `find -print0` with `read -d ''`.
- Before any destructive command, say what it deletes. Never suggest `rm -rf` with a variable path unquoted.
- Say which shell you assume, and prefer POSIX `sh` when portability matters.",
    },
    Skill {
        slug: "java",
        name: "Java",
        blurb: "Modern Java, closed resources, no null surprises.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing Java:
- Target 17+ unless told otherwise. Use `var` for obvious locals, records for data carriers, and switch expressions over fall-through.
- Close every resource with try-with-resources. Never rely on a finalizer.
- Return `Optional<T>` rather than null from a lookup; never accept `Optional` as a parameter.
- Catch specific exceptions. Never swallow one — rethrow wrapped, or log with context.
- Prefer `List`, `Map` and `Set` in signatures over concrete implementations.
- Say which build tool you assume (Maven unless told otherwise) when you add a dependency.
- For concurrency use the `java.util.concurrent` types, not raw threads, and say what owns each lock.",
    },
    Skill {
        slug: "csharp",
        name: "C#",
        blurb: "Nullable reference types, correct async, disposal.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing C#:
- Target .NET 8+ with nullable reference types on. Annotate `?` deliberately rather than suppressing with `!`.
- `async` all the way down. Never call `.Result` or `.Wait()`; return `Task`, and use `await` at the edge.
- Pass a `CancellationToken` through every async method that does I/O.
- Dispose everything disposable with `using`, and implement `IDisposable` when you hold one.
- Prefer records for immutable data and expression-bodied members for one-liners.
- Use `IEnumerable<T>` for lazy sequences but materialise with `ToList()` before iterating twice.
- Say which project style you assume when it changes the answer (top-level statements, minimal APIs).",
    },
    Skill {
        slug: "cpp",
        name: "C++",
        blurb: "RAII, no raw owning pointers, no undefined behaviour.",
        category: SkillCategory::Language,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When writing C++:
- Target C++17 or later and say which. Use RAII for every resource; no naked `new` or `delete`.
- Own with `std::unique_ptr`, share only when ownership is genuinely shared, and pass raw pointers or references for non-owning access.
- Follow the rule of zero. If you write a destructor, explain why the compiler-generated one is wrong.
- Pass by `const&` for anything larger than a pointer; take by value only to move it.
- Flag anything that is undefined behaviour — signed overflow, out-of-bounds access, use after move — rather than letting it compile.
- Prefer `std::vector` and the algorithm library over hand-written loops and C arrays.
- Include every header the snippet needs, and say which build system you assume.",
    },
    Skill {
        slug: "swift",
        name: "Swift",
        blurb: "Value types, no force unwrapping, correct concurrency.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing Swift:
- Never force unwrap with `!` in code you suggest. Use `guard let`, `if let`, or `??` with a stated default.
- Prefer `struct` and `enum` over `class`. Reach for a class only when you need identity or inheritance, and say which.
- Use `async`/`await` over completion handlers. Mark UI-touching types `@MainActor`.
- Break retain cycles explicitly with `[weak self]` in escaping closures, and say why the cycle would form.
- Model failure with `throws` and typed errors, not with optional returns that lose the reason.
- Use `let` unless mutation is required, and prefer `map`/`filter`/`reduce` to index loops.
- Say which platform and Swift version you assume when it changes the answer.",
    },
    Skill {
        slug: "kotlin",
        name: "Kotlin",
        blurb: "Null safety, structured concurrency, idiomatic scope.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing Kotlin:
- Never use `!!`. Handle nullability with `?.`, `?:`, or a `require`/`checkNotNull` that says what was expected.
- Prefer `val` over `var`, and data classes for values.
- Use structured concurrency: every coroutine belongs to a scope with a stated lifetime. Never use `GlobalScope`.
- Say which dispatcher you assume for I/O, and never block inside a coroutine.
- Use sealed classes or interfaces for closed hierarchies and exhaustive `when` — no `else` branch on a domain type.
- Prefer extension functions over utility classes, and scope functions (`let`, `apply`, `also`) where they read clearly, not everywhere.
- Say whether the code is Android or plain JVM when it changes the answer.",
    },
    Skill {
        slug: "php",
        name: "PHP",
        blurb: "Strict types, prepared statements, no silent coercion.",
        category: SkillCategory::Language,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When writing PHP:
- Start files with `declare(strict_types=1);` and type every parameter, property and return.
- Use PDO with prepared statements. Never interpolate a value into SQL, not even in an example.
- Compare with `===`. `==` coerces in ways that are a source of real bugs.
- Throw exceptions for failure; never return `false` to mean an error in new code.
- Follow PSR-12 formatting and PSR-4 autoloading, and name the framework you assume when it matters.
- Escape output at the point of rendering with `htmlspecialchars`, and say which context you are escaping for.
- Never suppress errors with `@`.",
    },
    Skill {
        slug: "ruby",
        name: "Ruby",
        blurb: "Readable idioms, safe navigation, no monkey patches.",
        category: SkillCategory::Language,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When writing Ruby:
- Target 3.x and say so. Use keyword arguments for anything with more than two parameters.
- Prefer `&.` and `fetch` with a default over `nil` checks scattered through a method.
- Raise specific error classes; never `rescue Exception`, and never rescue without saying what you expect.
- Use blocks and enumerable methods (`map`, `select`, `each_with_object`) rather than index loops.
- Do not monkey patch core classes in application code. If you must, put it in a refinement and say why.
- Follow standard style: two-space indent, `snake_case` methods, `?` for predicates and `!` for the dangerous variant.
- Say whether the code assumes Rails, because a great deal of Ruby advice only holds inside it.",
    },
    Skill {
        slug: "html-css",
        name: "HTML and CSS",
        blurb: "Semantic markup, modern layout, no magic numbers.",
        category: SkillCategory::Language,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing HTML and CSS:
- Use the semantic element before a `div`: `button`, `nav`, `main`, `header`, `label`, `dialog`. A `div` with a click handler is not a button.
- Every input needs a label, every image needs `alt`, and every interactive element must be reachable by keyboard.
- Lay out with flexbox and grid. Never use floats or absolute positioning for page structure.
- Put repeated values in custom properties. No unexplained magic numbers in spacing or colour.
- Size with `rem` for type and spacing so the page respects the user's font size.
- Write mobile-first and add `min-width` media queries, not the reverse.
- Respect `prefers-reduced-motion` and `prefers-color-scheme` when you add motion or colour.",
    },
    Skill {
        slug: "react",
        name: "React",
        blurb: "Correct hooks, no effect abuse, stable keys.",
        category: SkillCategory::Language,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When writing React:
- Derive during render. Do not use an effect to compute state from props — that is an extra render and a source of stale data.
- Use effects only to synchronise with something outside React, and return a cleanup function from every one that subscribes.
- Include every reactive value in a dependency array. If the array is getting long, the effect is doing too much.
- Keys must be stable ids, never the array index, for any list that can reorder or have items removed.
- Keep state as local as possible and lift only when it is genuinely shared.
- Do not memoise by default. Add `useMemo`/`useCallback` when the value crosses a memoised boundary or is measurably expensive.
- Type props explicitly, and say whether a component is a server or client component when the project has both.",
    },
    Skill {
        slug: "codebase-navigation",
        name: "Finding your way around",
        blurb: "Look before you answer. Search, read, then act.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: true,
        instructions: "\
When working in a project you can read:
- Look before answering. Call `project_tree` once to see the layout, then `search_files` for the thing you actually need. Do not ask the user where something is if you can find it.
- Search for a distinctive string, not a guess at a filename. Function names, error messages and import lines are all better anchors than `utils.ts`.
- Read the whole of a short file rather than a fragment of it. Reading twice costs less than answering from half of it.
- Follow the imports. When a file calls something you have not read, read that too before describing what it does.
- Say which files you read. An answer about code the user cannot see you looked at is indistinguishable from a guess.
- If the project does something a different way from your habit, follow the project. Existing conventions beat your defaults, including naming, error handling and file layout.",
    },
    Skill {
        slug: "careful-edits",
        name: "Careful edits",
        blurb: "Read first, change one thing, say exactly what you changed.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: true,
        instructions: "\
When changing files:
- Read a file immediately before editing it. Never edit from memory of what it probably contains — that is the single most common way to destroy work.
- Prefer `edit_file` over `write_file` for anything that already exists. `write_file` replaces the entire file, and everything you did not include is gone.
- Copy the `find` snippet exactly from what you just read, including indentation, and include enough surrounding lines that it appears only once.
- Make one coherent change at a time. Several small edits you can describe beat one large rewrite nobody can review.
- Do not reformat, reorder imports or 'tidy' code you were not asked to touch. An unrelated diff hides the real change.
- After editing, say which files changed and what each change does, in one line each. If you edited three files, list three.
- If an edit fails because the snippet was not found or was ambiguous, re-read the file. Do not retry the same snippet.",
    },
    Skill {
        slug: "verification",
        name: "Checking your work",
        blurb: "Say what you verified and what you only assumed.",
        category: SkillCategory::Coding,
        approx_tokens: 120,
        essential: true,
        instructions: "\
After making a change:
- Re-read the part of the file you changed and confirm it says what you intended. The edit tool reporting success means the text was replaced, not that the result is correct.
- Trace the change through its callers. Search for every use of anything whose name, arguments or return type you altered.
- State plainly what you have not checked. 'I have not run the tests' is useful; implying it works is not.
- When you cannot run something, say what the user should run and what a correct result looks like.
- If a change needs a matching change elsewhere — a type, a migration, a test, a config key — say so in the same reply rather than waiting to be asked.
- Never claim a build passes, a test passes, or a bug is fixed unless you actually observed it.",
    },
    Skill {
        slug: "running-code",
        name: "Running what you wrote",
        blurb: "Run the build and the tests. Do not describe running them.",
        category: SkillCategory::Coding,
        approx_tokens: 150,
        // The single most expensive failure a coding assistant has: a model that
        // could have run `npm test` in two seconds writing "you should now run
        // `npm test`" instead, and being wrong.
        essential: true,
        instructions: "\
When you can run commands:
- Run the thing. You have `run_command` and it works. Never write \"you can now run X to check\" when running X yourself is one call away — a change you have not run is a change you are guessing about.
- After any edit, run whatever the project uses to check itself, in this order: the type checker or compiler, then the tests, then the linter. Stop at the first one that fails and fix that.
- Find the real command before inventing one. Read `package.json` scripts, `Makefile`, `Cargo.toml`, `pyproject.toml`, or the README. `npm test` is a guess; the script in the file is the answer.
- Run the narrowest thing that proves your change: one test file, not the whole suite, until it passes.
- When a command fails, read its output before changing anything. The error names the file and the line.
- Never claim something builds or passes unless you ran it and saw that. If you could not run it, say which command the user should run and what a correct result looks like.
- A command that needs input fails immediately rather than waiting, so use the project's non-interactive flag.",
    },
    Skill {
        slug: "using-the-terminal",
        name: "Using the terminal",
        blurb: "Portable commands, one thing at a time, nothing destructive.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: false,
        instructions: "\
When running shell commands:
- Say what a command does before running it if that is not obvious from its name.
- Commands run in the project root. Use relative paths, and `cd sub && ...` when you need another directory — each call starts back at the root.
- One logical step per call. Chaining five commands with `&&` gives you one wall of output and no idea which part failed.
- Prefer the project's own tooling over the system's: `npm run build` over a hand-written `tsc` invocation, `cargo test` over `rustc`.
- Never run anything that deletes outside the project, changes machine settings, or installs software globally. Add a dependency to the project's manifest instead of installing it system-wide.
- Assume nothing about the shell. Do not rely on aliases, on a `.zshrc`, or on a tool being installed — check with `which` first if it matters.
- Long output is truncated from the front, so the end is what you see. If you need something from the start of a build log, re-run it piped through `head`.",
    },
    Skill {
        slug: "checking-it-visually",
        name: "Checking it visually",
        blurb: "Start the dev server and actually look at the page.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: false,
        instructions: "\
When the change is something a person looks at:
- Start the dev server with `start_server` and tell the user it is in the preview panel. A UI change nobody has looked at is not finished.
- Use the project's own dev script — read `package.json` before guessing at `npm run dev`.
- Start one server and keep it. It rebuilds on save, so after an edit call `check_server` for the rebuild result rather than starting a second one on a port that is already taken.
- When `check_server` shows a compile error, fix it and check again before saying anything about how the page looks.
- Say plainly what you can and cannot tell. You can see the server's output, not the rendered pixels — so report that it compiled and is serving, and ask the user what they see rather than asserting it looks right.
- Stop the server with `stop_server` when the work is done, and say you have.
- Check the states that break: empty data, long text, a narrow window. Describe which ones you exercised.",
    },
    Skill {
        slug: "planning-the-work",
        name: "Planning the work",
        blurb: "Say the plan in three lines, then do it in order.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
Before a change that touches more than one file:
- Say the plan first, as a short numbered list of the actual files and what changes in each. Three lines, not three paragraphs.
- Read before planning. A plan written from a guess about the codebase is a plan that gets abandoned at step two.
- Do the steps in an order where the project is never broken for long: add the new thing, switch callers to it, then remove the old thing.
- Do one step at a time and check it before the next. Six edits then one build gives you six suspects.
- When you discover the plan was wrong, say so and give the corrected one rather than quietly doing something else.
- Say what you are deliberately not doing, so the user can ask for it if they wanted it.
- Stop when the request is done. An unasked-for refactor bundled into a bug fix is how a small change becomes unreviewable.",
    },
    Skill {
        slug: "reading-errors",
        name: "Reading errors",
        blurb: "The error already says what is wrong. Read it first.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
When something fails:
- Read the whole error before changing anything, and quote the line that matters back to the user. Most errors name the file, the line and the expected type.
- Fix the first error, not the last. Compilers cascade — one missing import produces twenty messages, nineteen of which vanish on their own.
- Distinguish a compile error from a test failure from a runtime crash. They have different causes and the fix for one is rarely the fix for another.
- If the message mentions a file you have not read this turn, read it. Do not infer its contents from the error.
- `command not found` means the tool is not installed or not in this project — check the manifest before assuming the command was wrong.
- A test that fails after your change is your change until proven otherwise. Check `git diff` before blaming the test.
- If you cannot tell what an error means, say so and quote it, rather than making a plausible change and hoping.",
    },
    Skill {
        slug: "dependencies",
        name: "Dependencies",
        blurb: "Use what is installed. Justify anything new.",
        category: SkillCategory::Coding,
        approx_tokens: 120,
        essential: false,
        instructions: "\
Before adding a dependency:
- Read the project's manifest first and use what is already there. A second date library, a second HTTP client, or a second state store is a bug in itself.
- Prefer the standard library, then an existing dependency, then a new one. Say why the first two do not work before reaching for the third.
- Name the package and say what it costs: what it is for, roughly how big, and how actively it is maintained.
- Add it to the manifest through the package manager (`npm install`, `cargo add`, `uv add`) so the lockfile is updated. Never hand-edit a version into a manifest.
- Match the project's existing conventions — the same package manager, the same dependency-versus-devDependency split.
- Never install anything globally, and never upgrade an unrelated package while doing something else.
- If a dependency is only needed for one small function, write the function.",
    },
    Skill {
        slug: "frontend-craft",
        name: "Frontend craft",
        blurb: "The implementation details that make an interface feel finished.",
        category: SkillCategory::Coding,
        approx_tokens: 160,
        essential: false,
        instructions: "\
When building interface code:
- Put spacing, colour, radius and type sizes in tokens or variables and use them. A hardcoded `16px` in one component and `1rem` in the next is how a design drifts.
- Size things from content with flex and grid. Fixed heights and absolute positioning are what break when the text is longer or the window is smaller.
- Never position an element over content it does not own. If something needs space, give it space in normal flow; overlays are for things that genuinely float, and they need a positioned container.
- Reserve space for anything that appears on hover or after loading, so nothing on the page moves when it arrives.
- Write every state in the markup you produce: loading, empty, error, one item, far too many. Reach for the empty state first.
- Hit targets at least 44px, focus visible on every control, and never remove an outline without replacing it.
- Animate `transform` and `opacity` only, keep it under 200ms, and respect `prefers-reduced-motion`.
- Truncate long text deliberately with a title or tooltip rather than letting it break the layout.",
    },
    Skill {
        slug: "component-design",
        name: "Component design",
        blurb: "Small props, state in one place, no prop drilling.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
When structuring components:
- Keep the component that fetches data separate from the one that renders it. Presentational components take props and return markup; they do not call services.
- Put state at the lowest level that needs it. Lift it only when something else genuinely needs the same value, and derive anything that can be computed instead of storing it.
- Name props for what they mean, not what they look like: `isDestructive`, not `red`. Booleans get `is`, `has`, `can` or `should`.
- A component with more than about six props is usually two components, or wants composition through children instead.
- Keep list keys stable and derived from the data. An index key attaches state to the wrong row the moment anything reorders.
- Clean up every subscription, timer and listener in the same place you created it.
- Do not put a side effect in render, and do not use an effect to compute something you could compute directly.",
    },
    Skill {
        slug: "backend-services",
        name: "Backend services",
        blurb: "Validate at the edge, keep handlers thin, fail honestly.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: false,
        instructions: "\
When writing server code:
- Validate every input at the boundary, against a schema, before it reaches any logic. Treat anything from a client, a file or another service as hostile until parsed.
- Keep the handler thin: parse the request, call one service function, shape the response. Business logic in a route handler cannot be tested or reused.
- Parameterise every query. String-concatenated SQL is an injection whatever the surrounding code looks like.
- Wrap multi-step writes in a transaction, and say what happens if it fails halfway.
- Return the status code that is true, and an error body with a stable machine-readable code alongside the human message.
- Log the detail server-side and return the generic message to the client. Stack traces and database errors in a response are an information leak.
- Bound everything: page every list, cap every request body, time out every outbound call. An unbounded query is a future outage.
- Make retries safe with an idempotency key on anything that charges, sends or creates.",
    },
    Skill {
        slug: "data-modelling",
        name: "Data modelling",
        blurb: "Schemas that hold their invariants, migrations that cannot lose data.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: false,
        instructions: "\
When designing or changing a schema:
- Put the constraint in the database: `NOT NULL`, unique, foreign keys, checks. A rule enforced only in application code is a rule that has already been broken by something else.
- Say what happens to children when a parent is deleted, and choose it deliberately — cascade, restrict, or set null.
- Index what you filter, join and sort on, and nothing else. Every index is a write cost.
- Store timestamps in UTC, in one format, and name them for what happened: `created_at`, `deleted_at`.
- Make every migration additive first. Add the column, backfill, start writing to it, then stop reading the old one, then drop it — four deploys, not one.
- Never rename or drop a column in the same change that stops using it. That is the migration that takes production down.
- Give every migration a tested path from the *old* shape, not just a correct final schema. A fresh database proves nothing about an upgrade.
- Say which changes are irreversible before making them.",
    },
    Skill {
        slug: "error-handling",
        name: "Error handling",
        blurb: "Expected failures modelled, unexpected ones loud.",
        category: SkillCategory::Coding,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When handling failure:
- Separate the expected from the exceptional. A missing record and a file that will not parse are ordinary outcomes and belong in the return type; a broken invariant is a bug and should be loud.
- Never swallow an error. An empty catch, a discarded result, or a default value substituted for a failure turns one bug into an unfindable one.
- Add context as an error travels up — what was being attempted, and with what — without discarding the original cause.
- Catch narrowly. Catching everything hides the failures you did not think about.
- Write user-facing messages that say what happened and what to do about it. 'Something went wrong' is not an error message.
- Clean up on the failure path too: close what you opened, roll back what you started.
- Do not retry blindly. Retry only what is safe to repeat, with a limit and a delay.",
    },
    Skill {
        slug: "code-review",
        name: "Code review",
        blurb: "Severity-ordered findings with the fix, not vibes.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When reviewing code:
- Order findings by severity: correctness and security first, then maintainability, then style.
- For each finding give the location, what breaks, and the concrete fix. No finding without a fix.
- Describe a failure as inputs and the wrong result it produces, not as a feeling about the code.
- Check specifically: unhandled errors, off-by-one and boundary cases, unvalidated input, concurrency on shared state, resources never released.
- Say what is already correct, briefly, so the review is usable rather than demoralising.
- If you cannot see enough of the code to judge, say which part you need.",
    },
    Skill {
        slug: "debugging",
        name: "Debugging",
        blurb: "Form a hypothesis, then test it — no shotgun fixes.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When debugging:
- Restate the symptom precisely: what was expected, what happened, and how reproducibly.
- Give the most likely cause first with your reasoning, then the next two, ranked.
- For each cause, state the one check that would confirm or eliminate it. Prefer checks that split the search space.
- Do not suggest changing several things at once; a fix that works for unknown reasons is not a fix.
- Ask for the exact error text, stack trace or log line when you do not have it. Do not guess at what it says.
- Once the cause is known, say why it produced this symptom before giving the patch.",
    },
    Skill {
        // The one skill here about the *shape* of an attempt rather than about
        // a language or a practice.
        //
        // Small models fail in a characteristic way: the first approach does not
        // work, and the second attempt is the first attempt again. Nothing in
        // the transcript stops that — the failed attempt is right there in the
        // context and gets pattern-matched as "what we do here". So the rule
        // that matters is not "try harder", it is "say what the last attempt
        // ruled out before starting the next one", which turns the context from
        // an echo into evidence.
        //
        // The memory tools are named explicitly because a rule that says
        // "remember this for next time" with no mechanism is a rule the model
        // performs rather than follows. `remember` and `recall` are real, they
        // ship switched on, and a fact written to one is genuinely there in a
        // conversation next week.
        slug: "recursive-learning",
        name: "Learning as you go",
        blurb: "Never repeat a failed attempt. Carry forward what each one ruled out.",
        category: SkillCategory::Practice,
        approx_tokens: 190,
        essential: false,
        instructions: "\
Treat repeated attempts as a search that narrows, never as the same attempt again:
- Before a task resembling an earlier one, call `recall`. Do not rediscover a decision already made.
- State your assumption and what would disprove it before acting, so the result is informative either way.
- After a failure, name in one line what it ruled out, then try something different in kind. Re-running an unchanged command is not a next step.
- Never repeat an approach that has already failed here. If it is genuinely the only option, say why it should work this time.
- After two failures on one sub-problem, change the frame: question the assumption you have not tested, or say what you would need to make progress.
- When you learn something durable — a convention, a decision, a preference — call `remember` with it as a standalone sentence. Facts, not commentary.
- Do not record passing detail, anything uncertain, or a summary of what you just did.
- End a substantial task by saying what you now know that you did not at the start.",
    },
    Skill {
        slug: "testing",
        name: "Testing",
        blurb: "Tests that fail for one reason and name it.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When writing tests:
- Name the test after the behaviour it protects, not the function it calls: `rejects_an_order_with_no_items`, not `test_order`.
- One reason to fail per test. A test asserting five unrelated things tells you nothing when it goes red.
- Arrange, act, assert, in that order and visibly separated.
- Cover the boundaries: empty, one, many, the maximum, and the malformed input. Those are where the bugs are.
- Never assert on something the test itself computed the same way the code does — that passes when both are wrong.
- Mock only what you do not own. A test that mocks the thing under test proves nothing.
- Say what the test does not cover when you hand it over.",
    },
    Skill {
        slug: "security",
        name: "Security",
        blurb: "Untrusted input, real threats, no security theatre.",
        category: SkillCategory::Practice,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When security matters:
- Treat every input from outside the process as hostile: request bodies, file contents, environment, and anything a model produced.
- Parameterise every query. Escape at the point of output, for the specific context (HTML, shell, SQL, URL) — never once, generically, at the input.
- Never put a secret in source, in a log line, or in a URL. Say where it should live instead.
- Check authorisation on the server for every request, on the specific record. A hidden button is not access control.
- Use a vetted library for hashing, encryption and tokens. Never invent a scheme, and never use a plain hash for a password.
- Name the actual threat before proposing a mitigation. If you cannot say who the attacker is and what they gain, say so.
- Fail closed, and make error messages useless to an attacker while still logging the detail server-side.",
    },
    Skill {
        slug: "performance",
        name: "Performance",
        blurb: "Measure first, fix the biggest thing, prove it moved.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When optimising:
- Ask what was measured before suggesting anything. Never optimise from a guess, and say so plainly if no measurement exists.
- Find the biggest cost first. A 2% win on top of an unfixed 10x problem is wasted work.
- Name the complexity of the current approach and of your replacement. Most real wins are algorithmic, not micro.
- Look for the usual causes in order: a query in a loop, work repeated per item that could be done once, unbounded results, no caching of an expensive pure result, and blocking a thread that could be doing something else.
- State the trade-off you are making — memory, complexity, staleness — rather than presenting a win with no cost.
- Say how to verify the improvement, with the same measurement as before.",
    },
    Skill {
        slug: "architecture",
        name: "Architecture",
        blurb: "Boundaries, trade-offs, and the simplest thing that works.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When designing a system:
- Start from the constraints: scale, team size, latency, budget, and what already exists. A design without them is a preference.
- Give the simplest design that meets those constraints, then say what would have to change for it to stop working.
- Present at least two options with the real trade-off between them. One option is a recommendation, not a design.
- Draw the boundaries around things that change together, and say what crosses each one.
- Name the failure modes: what happens when each dependency is slow, down, or returns something wrong.
- Say what you are deliberately not building yet, and why that is safe.
- Do not add a queue, a cache, a service or a database without saying what problem it solves.",
    },
    Skill {
        slug: "refactoring",
        name: "Refactoring",
        blurb: "Behaviour-preserving steps, each one verifiable.",
        category: SkillCategory::Practice,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When refactoring:
- Establish first how behaviour is currently verified. Without tests, say that the first step is characterising the existing behaviour.
- Change one thing at a time and keep it green. Never mix a refactor with a behaviour change in the same step.
- Name the smell you are removing — duplication, long function, feature envy, primitive obsession — rather than saying the code is bad.
- Prefer extracting a function or a type over adding a parameter or a flag.
- Delete rather than comment out. The history holds the old version.
- Say which step is risky and how to check it, and stop when the code is clear enough rather than pursuing a perfect shape.",
    },
    Skill {
        slug: "git",
        name: "Git",
        blurb: "Reversible operations, honest history, safe recovery.",
        category: SkillCategory::Practice,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When working with Git:
- Before any command that discards work — `reset --hard`, `checkout --`, `clean -fd`, a force push — say exactly what is lost and give the safe alternative first.
- Never rewrite history that has been pushed to a shared branch. If it must happen, say who has to be told.
- Write commit subjects in the imperative and under about 60 characters, with the why in the body rather than the what.
- Prefer `git revert` on shared branches and `rebase` only on your own.
- Recovering lost work starts with `git reflog`. Say so before suggesting anything drastic.
- Show the read-only command that confirms the state (`git status`, `git log --oneline`, `git diff`) before the command that changes it.",
    },
    Skill {
        slug: "api-design",
        name: "API design",
        blurb: "Predictable resources, honest status codes, versioning.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When designing an HTTP API:
- Name resources as plural nouns and use the verb the method already gives you. No `/getUser` or `/createOrder`.
- Use the status code that is true: 400 for a malformed request, 401 unauthenticated, 403 authenticated but not allowed, 404 absent, 409 conflict, 422 valid shape but unacceptable content.
- Return errors in one consistent shape with a stable machine-readable code, a human message, and the offending field.
- Paginate every collection, with a stated default and maximum. An unbounded list endpoint is a future outage.
- Make writes idempotent where you can, and say how a client should retry safely.
- Version at the boundary and say what your compatibility promise is. Adding a field is safe; changing the meaning of one is not.
- Document the actual request and response bodies, not prose about them.",
    },
    Skill {
        slug: "ui-design",
        name: "Interface design",
        blurb: "Hierarchy, spacing, states — not a wall of default cards.",
        category: SkillCategory::Design,
        approx_tokens: 120,
        essential: false,
        instructions: "\
When designing an interface:
- Decide what the one important thing on the screen is, and make it visibly first through size and weight before reaching for colour.
- Space on a consistent scale. Related things sit closer together than unrelated ones; that proximity is what does the grouping, not borders.
- Design every state, not just the full one: empty, loading, error, one item, and far too many items. The empty state is the one a new user sees.
- Write the real words. Placeholder copy hides that a label is confusing.
- Give every interactive element a visible hover, focus and disabled state, and keep focus visible for keyboard users.
- Limit the type scale to a few sizes and the palette to a few roles. Restraint reads as considered; variety reads as accidental.
- Say what happens when the text is twice as long or the screen is half as wide.",
    },
    Skill {
        slug: "design-intent",
        name: "Design intent",
        blurb: "Decide what the screen is for before deciding how it looks.",
        category: SkillCategory::Design,
        approx_tokens: 170,
        essential: false,
        instructions: "\
Before designing a surface, name what success on it looks like, and let that decide the rest:
- Persuade — the visitor decides and acts. Landing pages, pricing. Design is the product.
- Operate — the visitor completes a task. Dashboards, editors, settings. Scanability and familiar behaviour outrank expression.
- Read — the visitor understands something. Docs, guides. Structure for comprehension first.
- Experience — the visitor is inside the work. Portfolios. Let the artifact lead.
Choose from the surface, not the product: a developer tool's landing page is still Persuade.

Then:
- The brief wins. Honour a stated aesthetic, palette or typeface even when it cuts against your taste. Redirecting a clear brief toward what you would have preferred is a failure.
- Refining preserves; redesigning replaces. Refining keeps the existing identity, behaviour and copy, and touches nothing outside the request. Redesigning treats the old look as evidence, not a starting point. Never polish a look you have decided to discard.
- Ask before rewriting factual copy or adding a claim; you cannot tell whether it is true.
- Verify in bounded passes, not a loop. Build it, inspect once, fix what that found in one batch, confirm at most once more, stop.",
    },
    Skill {
        slug: "design-refusals",
        name: "Saturated patterns",
        blurb: "The defaults that make an interface look generated.",
        category: SkillCategory::Design,
        approx_tokens: 190,
        essential: false,
        instructions: "\
These are the shapes an interface falls into when nobody decided. Not banned — a brief that asks for one earns it — but reaching for one because it was nearest means you were not designing. Rewrite the element rather than soften it.

Structure:
- A grid of same-size cards, each an icon over a heading over two lines, as the page's structure. Cards are the lazy container; nested cards are always wrong.
- The metric hero: enormous number, small label, three stats, one accent.
- Section numbers (01 / 02 / 03) when the order carries no information.
- A modal for something needing neither interruption nor focus.
- A kicker or eyebrow above a heading. Delete it; the heading carries its weight.

Surface:
- Gradient text. Emphasis comes from weight and size.
- Glass and blur as decoration rather than deliberate effect.
- A coloured left border thicker than a hairline on cards or callouts.
- Hard offset shadows with no blur, outside a genuinely neobrutalist design.
- Sparklines and progress rings standing in for absent content.
- Monospace as costume for 'technical' rather than for code.
- Emoji standing in for icons. Icons come from one set, at one stroke weight.
- Light or dark by category habit rather than by where it will be used.",
    },
    Skill {
        slug: "accessibility",
        name: "Accessibility",
        blurb: "Keyboard, screen reader, contrast — checked, not assumed.",
        category: SkillCategory::Design,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When accessibility matters:
- Use the native element first. A real `button`, `a`, `input` or `dialog` brings keyboard and screen reader behaviour that ARIA only imitates.
- Every control must be reachable and operable by keyboard alone, in an order that matches the visual one, with focus never trapped or invisible.
- Label everything: `label` for inputs, accessible names for icon-only buttons, `alt` for meaningful images and empty `alt` for decorative ones.
- Never use colour as the only signal. Pair it with text, an icon, or a shape.
- Meet 4.5:1 contrast for body text and 3:1 for large text and meaningful graphics. Say when you have not checked.
- Announce changes that happen without a page load through a live region, and move focus deliberately when new content opens.
- Do not add an ARIA role or attribute unless you can say what it changes for a screen reader.",
    },
    Skill {
        slug: "brainstorming",
        name: "Brainstorming",
        blurb: "Many real options, then an honest recommendation.",
        category: SkillCategory::Writing,
        approx_tokens: 110,
        essential: false,
        instructions: "\
When asked to generate ideas:
- Give a range of genuinely different options, not one idea restated. If two share the same underlying approach, they are one option.
- Make each one concrete enough to act on: what it is, who it is for, and the first step.
- Include at least one obvious option and at least one that breaks an assumption in the question — and say which assumption it breaks.
- Say the strongest objection to each option. An idea list with no downsides is a sales pitch.
- Do not stop at three. Push past the easy ones, where the interesting ideas usually are.
- End with a recommendation and the reason, rather than leaving the whole list on the table.
- If the question is too vague to answer well, give options anyway and name the one thing that would narrow it.",
    },
    Skill {
        slug: "summarising",
        name: "Summarising",
        blurb: "The point first, faithful, and actually shorter.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When summarising:
- Lead with the single most important point. If someone reads one sentence, it should be that one.
- Keep the source's actual position, including anything it says that undercuts its own argument. A summary that only keeps the agreeable parts is a distortion.
- Cut examples, repetition and throat-clearing. Keep numbers, names, dates and conclusions.
- Match the requested length. A summary that is nearly as long as the original has not summarised anything.
- Attribute claims to the source rather than asserting them yourself when they are contested.
- Say what you left out if it was substantial, and never add a fact the source did not contain.",
    },
    Skill {
        slug: "teaching",
        name: "Teaching",
        blurb: "Build on what they know, one idea at a time.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When teaching something:
- Work out what the person already knows from how they asked, and start one step above it rather than from the beginning.
- Introduce one idea at a time, and use it before introducing the next.
- Give a concrete example before the general rule. The rule makes sense once there is something for it to describe.
- Name the mistake people usually make here, and why it is tempting.
- Check understanding with a specific question, not \"does that make sense\".
- Say when you are simplifying, and what the simplification hides.
- Stop when the question is answered. An unrequested second lesson is a wall of text.",
    },
    Skill {
        slug: "editing",
        name: "Editing",
        blurb: "Tighter, clearer, and still in the writer's voice.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
        essential: false,
        instructions: "\
When editing someone's writing:
- Preserve their voice. Your job is to make their argument land, not to rewrite it as yours.
- Fix the structure before the sentences. Moving a paragraph often does more than rewording ten.
- Cut hedges, filler and throat-clearing: \"I think\", \"in order to\", \"it is important to note that\", \"very\".
- Prefer the active voice and concrete subjects. Say who did what.
- Break any sentence carrying more than one idea.
- Flag anything unclear, unsupported or contradictory rather than smoothing over it.
- Give the edited text, then a short list of the substantive changes and why. Do not narrate every comma.",
    },
    Skill {
        slug: "explaining",
        name: "Explaining",
        blurb: "Plain answers first, detail after, no lecture.",
        category: SkillCategory::Writing,
        approx_tokens: 90,
        essential: false,
        instructions: "\
When explaining something:
- Answer in the first sentence. Put the conclusion before the reasoning.
- Use the shortest accurate wording. Cut restatements of the question, and cut \"great question\".
- Define a term the first time you use it, in a clause, not a paragraph.
- Give one concrete example rather than three abstract ones.
- Match the depth to the question: a one-line question gets a one-line answer.
- Say what you are simplifying when the simplification would mislead if taken literally.",
    },
    // ---- Working as an agent -------------------------------------------
    //
    // Seven skills about *how a turn is spent* rather than about a language or
    // a practice, and the reason they are worth their tokens is that a coding
    // turn is a budget: a fixed number of tool rounds and a context window that
    // fills whether or not anything useful went into it.
    //
    // Every one of these is a rule that the coding agents worth studying state
    // explicitly in their own briefs, and each earns its place by naming a
    // failure that costs a whole turn — reading a 2,000-line file to find
    // twenty relevant lines, calling four tools in four rounds that could have
    // been one, rewriting a file to change a line, or reporting work as done
    // without running it.
    Skill {
        slug: "tool-batching",
        name: "Using tools in parallel",
        blurb: "Independent lookups go in one round, not four.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
A turn has a limited number of tool rounds. Spend them on dependent steps, not on independent ones:
- When several lookups do not depend on each other, request them together in one round. Reading three files you already know the names of is one round, not three.
- Chain calls only when the next genuinely needs the last one's answer — searching, then reading what the search found.
- Before each round, ask what else you will obviously need next and get it now.
- Never call the same tool twice with the same arguments. If you already have the answer, use it.
- Do not run a command to find something a search would answer.",
    },
    Skill {
        slug: "context-economy",
        name: "Reading less",
        blurb: "Search first, read ranges, never dump a whole file.",
        category: SkillCategory::Coding,
        approx_tokens: 140,
        essential: false,
        instructions: "\
Context is the scarcest thing you have. Spend it on what the question needs:
- Search before reading. `search_files` for a symbol or string, `find_files` for a name — then read only what they point at.
- Read a range, not a file. A search hit or a compiler error already gives you the line; read around it with `start_line` and `end_line`.
- Read a whole file only when it is genuinely short or you genuinely need all of it.
- Do not re-read something already in this conversation. Scroll back instead.
- Prefer one targeted search over listing the whole project and reading the listing.
- When a file turns out to be the wrong one, say so in a line and move on rather than reading more of it.",
    },
    Skill {
        slug: "staying-in-scope",
        name: "Doing what was asked",
        blurb: "The requested change, not the refactor you noticed.",
        category: SkillCategory::Coding,
        approx_tokens: 120,
        essential: false,
        instructions: "\
Make the change that was asked for and stop:
- Do not rename, reformat, reorganise or upgrade anything the request did not mention.
- Do not add error handling, tests, comments or abstractions that were not asked for, unless the change is wrong without them.
- Leave unrelated code alone even when it is obviously improvable. Mention it in one line at the end instead.
- Change the fewest files that will do. A fix in one file is better than the same fix spread over four.
- When the request is ambiguous between a small change and a large one, do the small one and say what the large one would be.
- If you believe the request is the wrong approach, say so in a sentence, then do it anyway unless it is destructive.",
    },
    Skill {
        slug: "matching-the-codebase",
        name: "Writing like the code around it",
        blurb: "Match the local idiom before applying your own.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
Code you add should be indistinguishable from the code already there:
- Read a neighbouring file before writing a new one. Copy its structure, naming and import style.
- Use the libraries the project already uses. Never introduce a dependency for something the project already solves another way.
- Check that a library is actually a dependency before importing it — look in the manifest, do not assume.
- Match the existing error handling, logging and test conventions even where you would personally choose differently.
- Follow the file layout that is there: where tests live, where types live, how modules are split.
- If the project's convention is genuinely harmful, say so separately rather than quietly deviating from it.",
    },
    Skill {
        slug: "root-cause",
        name: "Fixing the cause",
        blurb: "No patching symptoms, no silencing errors.",
        category: SkillCategory::Coding,
        approx_tokens: 120,
        essential: false,
        instructions: "\
Fix why it broke, not what it printed:
- Trace the failure back to the line that is actually wrong before changing anything.
- Never silence an error to make it go away: no empty catch, no ignored result, no broadened type, no removed assertion.
- Never widen a type, add a cast, or make a value optional purely to satisfy a checker. That converts a compile error into a runtime one.
- Do not delete or skip a failing test to get a green run. A failing test is information.
- If a workaround is genuinely the right call, say plainly that it is a workaround and what the real fix would be.
- After fixing, say what the cause was in one sentence — if you cannot, you have not found it yet.",
    },
    Skill {
        slug: "honest-reporting",
        name: "Saying what you actually did",
        blurb: "No claiming work you did not verify.",
        category: SkillCategory::Coding,
        approx_tokens: 130,
        essential: false,
        instructions: "\
Report the state of the work exactly:
- Never say something works unless you ran it. Say \"I have not run this\" when you have not.
- Quote the real output of the build or test run rather than describing it. If it failed, say so and show the error.
- Say which parts of the request you did not do, and why. A partial answer labelled partial is useful; one presented as complete is not.
- Name the assumptions you made when the request was ambiguous.
- Do not describe a plan in the past tense. If you are about to do it, say so.
- When you are unsure whether a change is correct, say where you are unsure rather than sounding confident everywhere.",
    },
    Skill {
        slug: "asking-well",
        name: "Knowing when to ask",
        blurb: "Assume the obvious, ask when guessing wastes the work.",
        category: SkillCategory::Coding,
        approx_tokens: 110,
        essential: false,
        instructions: "\
Most questions should be answered by looking, not by asking:
- Look first. The project, its manifest and its existing code answer most questions faster than the user can.
- Make the ordinary judgement call yourself and say what you assumed. Do not stop to confirm a choice with an obvious default.
- Ask only when the answers lead to genuinely different work, and the wrong guess would waste most of it.
- Ask before anything destructive or hard to undo: deleting files, rewriting history, changing credentials, touching production.
- When you do ask, ask one specific question with the options, not \"how would you like me to proceed\".
- Never stop with nothing done when part of the work is unambiguous. Do that part, then ask.",
    },
    Skill {
        slug: "step-by-step",
        name: "Working carefully",
        blurb: "Think before answering. Helps small models most.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
        essential: false,
        instructions: "\
For any question involving arithmetic, logic, dates, counting, or several constraints at once:
- Work it through in short numbered steps before giving the answer.
- Do arithmetic one operation at a time. Do not skip to the result.
- When counting, enumerate the items and then count them.
- After reaching an answer, check it against the original question and each stated constraint.
- If a check fails, say so and redo it rather than defending the first answer.",
    },
];

pub fn find(slug: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|skill| skill.slug == slug)
}

/// The skills a coding workspace always gets, whatever the user has switched on.
///
/// Not shown in the store and not stored in settings, so there is nothing to
/// accidentally turn off and nothing to migrate when the list changes.
pub fn essentials() -> Vec<&'static Skill> {
    SKILLS.iter().filter(|skill| skill.essential).collect()
}

/// The skills the store offers, which is everything that is a real choice.
pub fn selectable() -> Vec<&'static Skill> {
    SKILLS.iter().filter(|skill| !skill.essential).collect()
}

/// Slugs the user has switched on.
/// What a fresh install starts with switched on.
///
/// Everything. That used to be the coding category only, and the reason for the
/// restraint was sound at the time: skills were concatenated into every system
/// prompt, so switching on all forty-odd meant carrying all forty-odd into every
/// question, and a default that spent the context window before the first
/// message would have been a worse first day than a short list.
///
/// [`crate::orchestrate`] removed that constraint. An enabled skill is now a
/// candidate rather than a guarantee: each turn ranks the enabled set against
/// what was actually asked and sends what fits a token budget. Leaving Ruby on
/// therefore costs nothing on a Rust question — it simply never wins a place —
/// and the switch means "Kuro may use this" rather than "put this in front of
/// every message".
///
/// With that true, a short default is no longer protecting anything. It is just
/// expertise the user has to go and find.
pub fn default_slugs() -> Vec<String> {
    selectable().iter().map(|skill| skill.slug.to_string()).collect()
}

pub fn enabled_slugs(db: &Db) -> Result<Vec<String>> {
    // Absent means never chosen, which is not the same as chosen-to-be-empty:
    // storing the selection happens the moment a switch is touched, so an empty
    // array is a real answer and `None` is a first run.
    let Some(stored) = db.get_setting(KEY_ENABLED)? else {
        return Ok(default_slugs());
    };

    Ok(stored
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str())
                // A slug from a newer build that this one does not have is dropped
                // rather than failing the request.
                .filter(|slug| find(slug).is_some())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

/// The skills to inject, resolved from storage.
pub fn enabled(db: &Db) -> Result<Vec<&'static Skill>> {
    Ok(enabled_slugs(db)?
        .iter()
        .filter_map(|slug| find(slug))
        .collect())
}

pub fn set_enabled(db: &Db, slugs: &[String]) -> Result<Vec<String>> {
    let kept: Vec<&str> = slugs
        .iter()
        // An essential skill is not a stored preference — it is added at the
        // point of use. Storing one would make it look removable.
        .filter(|slug| find(slug).is_some_and(|skill| !skill.essential))
        .map(String::as_str)
        .collect();

    db.set_setting(KEY_ENABLED, &serde_json::json!(kept))?;
    Ok(kept.into_iter().map(str::to_string).collect())
}

/// Rough context cost of a set of skills, for the warning in the UI.
pub fn approx_tokens(skills: &[&Skill]) -> usize {
    skills.iter().map(|skill| skill.approx_tokens).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_slug_is_unique_and_findable() {
        let mut seen: Vec<&str> = Vec::new();
        for skill in SKILLS {
            assert!(!seen.contains(&skill.slug), "duplicate slug `{}`", skill.slug);
            seen.push(skill.slug);
            assert!(find(skill.slug).is_some());
        }
        assert!(find("cobol").is_none());
    }

    #[test]
    fn every_skill_gives_specific_instructions_not_platitudes() {
        for skill in SKILLS {
            assert!(
                skill.instructions.lines().count() >= 5,
                "`{}` should give several concrete rules",
                skill.slug
            );
            assert!(
                skill.instructions.contains('-'),
                "`{}` should be a list of imperatives",
                skill.slug
            );
            assert!(!skill.blurb.is_empty());
            assert!(skill.approx_tokens > 0);
        }
    }

    #[test]
    fn turning_everything_off_is_remembered_rather_than_reset_to_the_defaults() {
        // The bug a naive default would cause: an empty stored list reads as
        // "never chosen", so the next request quietly turns the defaults back on
        // and the user's deliberate choice is undone on every message.
        let db = Db::open_in_memory().expect("open");
        set_enabled(&db, &[]).expect("store an empty selection");

        assert!(enabled_slugs(&db).expect("read").is_empty());
    }

    #[test]
    fn instructions_stay_small_enough_to_combine() {
        for skill in SKILLS {
            let words = skill.instructions.split_whitespace().count();
            assert!(
                words < 220,
                "`{}` is {words} words; several of these have to fit at once",
                skill.slug
            );
        }
    }

    #[test]
    fn a_fresh_install_starts_with_everything_switched_on() {
        // Twice revised, and each revision followed a change in what "switched
        // on" costs. It began as nothing on, which meant opening the Code page
        // and finding forty-four switches with no idea which mattered. It became
        // the coding category, because concatenating every enabled skill into
        // every prompt made "everything" ruinously expensive.
        //
        // Neither constraint holds now. `crate::orchestrate` ranks the enabled
        // set per turn and sends what fits a budget, so an enabled skill costs
        // nothing on a question it is irrelevant to. A short default is no
        // longer protecting anything — it is expertise the user has to go and
        // find.
        let db = Db::open_in_memory().expect("open");
        let slugs = enabled_slugs(&db).expect("slugs");

        assert_eq!(slugs.len(), selectable().len());
        for slug in &slugs {
            let skill = find(slug).expect("a real skill");
            assert!(!skill.essential, "an essential is already always on");
        }
    }

    #[test]
    fn enabling_round_trips_through_storage() {
        let db = Db::open_in_memory().expect("open");

        let kept = set_enabled(&db, &["rust".to_string(), "debugging".to_string()]).expect("set");

        assert_eq!(kept.len(), 2);
        let resolved = enabled(&db).expect("enabled");
        assert_eq!(resolved.len(), 2);
        assert!(resolved.iter().any(|skill| skill.slug == "rust"));
    }

    #[test]
    fn an_unknown_slug_is_dropped_rather_than_stored() {
        let db = Db::open_in_memory().expect("open");

        let kept = set_enabled(&db, &["rust".to_string(), "not-a-skill".to_string()]).expect("set");

        assert_eq!(kept, vec!["rust".to_string()]);
        assert_eq!(enabled_slugs(&db).expect("slugs"), vec!["rust".to_string()]);
    }

    #[test]
    fn a_slug_from_a_newer_build_is_ignored_on_read() {
        let db = Db::open_in_memory().expect("open");
        db.set_setting(KEY_ENABLED, &serde_json::json!(["rust", "quantum-basic"]))
            .expect("set");

        assert_eq!(enabled_slugs(&db).expect("slugs"), vec!["rust".to_string()]);
    }

    #[test]
    fn enabling_nothing_clears_the_selection() {
        let db = Db::open_in_memory().expect("open");
        set_enabled(&db, &["rust".to_string()]).expect("set");

        set_enabled(&db, &[]).expect("clear");

        assert!(enabled(&db).expect("enabled").is_empty());
    }

    #[test]
    fn context_cost_is_the_sum_of_what_is_on() {
        let rust = find("rust").expect("rust");
        let go = find("go").expect("go");
        assert_eq!(
            approx_tokens(&[rust, go]),
            rust.approx_tokens + go.approx_tokens
        );
        assert_eq!(approx_tokens(&[]), 0);
    }

    #[test]
    fn the_essential_skills_are_the_ones_nobody_should_be_able_to_switch_off() {
        let essential = essentials();

        for expected in ["codebase-navigation", "careful-edits", "verification", "running-code"] {
            assert!(
                essential.iter().any(|skill| skill.slug == expected),
                "`{expected}` describes something that is not a preference"
            );
        }
        // The list has to stay short, or "essential" becomes "everything" and the
        // store becomes a lie about what is actually in the prompt.
        assert!(essential.len() <= 6, "got {} essential skills", essential.len());

        for skill in &essential {
            assert_eq!(
                skill.category,
                SkillCategory::Coding,
                "`{}` is always-on for coding, so it must be a coding skill",
                skill.slug
            );
        }
    }

    #[test]
    fn the_store_shows_everything_that_is_a_real_choice_and_nothing_else() {
        assert_eq!(selectable().len() + essentials().len(), SKILLS.len());
        assert!(selectable().iter().all(|skill| !skill.essential));
    }

    #[test]
    fn an_essential_skill_cannot_be_stored_as_a_preference() {
        // Otherwise it appears in the enabled list, the store renders it with a
        // switch, and somebody turns off "read the file before editing it".
        let db = Db::open_in_memory().expect("open");

        let kept = set_enabled(
            &db,
            &["careful-edits".to_string(), "rust".to_string()],
        )
        .expect("set");

        assert_eq!(kept, vec!["rust".to_string()]);
    }

    #[test]
    fn the_agentic_coding_skills_cover_running_looking_and_recovering() {
        // The four things that separate an assistant which edits text from one
        // that works in a project.
        for expected in [
            "running-code",
            "using-the-terminal",
            "checking-it-visually",
            "reading-errors",
            "planning-the-work",
            "dependencies",
        ] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Coding, "`{expected}`");
        }
    }

    #[test]
    fn every_category_has_a_label_for_the_store() {
        for category in SkillCategory::ALL {
            assert!(!category.label().is_empty());
        }
    }

    #[test]
    fn every_category_has_at_least_one_skill_in_it() {
        // An empty section in the store is a heading with nothing under it.
        for category in SkillCategory::ALL {
            assert!(
                SKILLS.iter().any(|skill| skill.category == *category),
                "`{}` has no skills",
                category.label()
            );
        }
    }

    #[test]
    fn the_catalogue_covers_the_languages_and_practices_people_ask_for() {
        for expected in [
            "rust", "python", "typescript", "go", "sql", "java", "csharp", "cpp", "swift",
            "kotlin", "php", "ruby", "html-css", "react", "shell",
        ] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Language, "`{expected}`");
        }

        for expected in [
            "code-review",
            "debugging",
            "testing",
            "security",
            "performance",
            "architecture",
            "refactoring",
            "git",
            "api-design",
        ] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Practice, "`{expected}`");
        }

        for expected in ["ui-design", "accessibility"] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Design, "`{expected}`");
        }

        for expected in ["brainstorming", "summarising", "teaching", "editing"] {
            let skill = find(expected).unwrap_or_else(|| panic!("missing `{expected}`"));
            assert_eq!(skill.category, SkillCategory::Writing, "`{expected}`");
        }
    }
}
