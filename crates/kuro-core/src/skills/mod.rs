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
//! Skills are deliberately not auto-selected. Guessing which expertise a message
//! needs is a classification problem that would be wrong often enough to be
//! annoying, and being wrong here means silently changing how the model answers.

use serde::Serialize;

use crate::db::Db;
use crate::Result;

/// Settings key holding the slugs the user switched on.
pub const KEY_ENABLED: &str = "skills.enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCategory {
    Language,
    Practice,
    Design,
    Writing,
}

impl SkillCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Language => "Languages",
            Self::Practice => "Engineering practice",
            Self::Design => "Interface and design",
            Self::Writing => "Writing and reasoning",
        }
    }

    /// Every category, in the order the store shows them.
    pub const ALL: &'static [SkillCategory] = &[
        Self::Language,
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
        slug: "code-review",
        name: "Code review",
        blurb: "Severity-ordered findings with the fix, not vibes.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
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
        slug: "testing",
        name: "Testing",
        blurb: "Tests that fail for one reason and name it.",
        category: SkillCategory::Practice,
        approx_tokens: 110,
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
        slug: "accessibility",
        name: "Accessibility",
        blurb: "Keyboard, screen reader, contrast — checked, not assumed.",
        category: SkillCategory::Design,
        approx_tokens: 110,
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
        instructions: "\
When explaining something:
- Answer in the first sentence. Put the conclusion before the reasoning.
- Use the shortest accurate wording. Cut restatements of the question, and cut \"great question\".
- Define a term the first time you use it, in a clause, not a paragraph.
- Give one concrete example rather than three abstract ones.
- Match the depth to the question: a one-line question gets a one-line answer.
- Say what you are simplifying when the simplification would mislead if taken literally.",
    },
    Skill {
        slug: "step-by-step",
        name: "Working carefully",
        blurb: "Think before answering. Helps small models most.",
        category: SkillCategory::Writing,
        approx_tokens: 100,
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

/// Slugs the user has switched on.
pub fn enabled_slugs(db: &Db) -> Result<Vec<String>> {
    let Some(stored) = db.get_setting(KEY_ENABLED)? else {
        return Ok(Vec::new());
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
        .filter(|slug| find(slug).is_some())
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
    fn nothing_is_enabled_on_a_fresh_install() {
        let db = Db::open_in_memory().expect("open");
        assert!(enabled_slugs(&db).expect("slugs").is_empty());
        assert!(enabled(&db).expect("skills").is_empty());
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
