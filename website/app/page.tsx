const shippedCapabilities = [
  {
    number: "01",
    title: "Compiler pipeline",
    description:
      "Lexing, parsing, semantic analysis, typed resolution, and execution remain independently testable boundaries.",
    proof: "Source-mapped diagnostics · typed HIR · explicit failures",
  },
  {
    number: "02",
    title: "Project-scale Apex",
    description:
      "Ordinary SFDX projects compile across files with class hierarchies, member resolution, and incremental reuse.",
    proof: "SFDX discovery · dependency graphs · cached builds",
  },
  {
    number: "03",
    title: "Deterministic tests",
    description:
      "Apex tests run in isolated interpreters with bounded parallelism, stable ordering, and CI-ready reporting.",
    proof: "@IsTest · JUnit · line and branch coverage",
  },
];

const roadmap = [
  {
    milestone: "M1–M15",
    state: "Complete",
    title: "Core local development loop",
    description:
      "Language, projects, tests, local data, platform services, editor workflows, enterprise CI, and hybrid validation.",
  },
  {
    milestone: "M16–M17",
    state: "Complete",
    title: "Phase 2 evidence entry",
    description:
      "Checked conditional/runtime-type expressions plus reviewed candidate-bound live Salesforce validation evidence.",
  },
  {
    milestone: "S0",
    state: "Complete",
    title: "Stabilization gate",
    description:
      "Process safety, explicit instrumentation, runtime graph safety, execution context, and enforceable release gates.",
  },
  {
    milestone: "M18",
    state: "Complete",
    title: "Null-aware expressions",
    description:
      "Checked safe navigation and null coalescing with lazy, evaluate-once execution and branch coverage.",
  },
  {
    milestone: "M19–M21",
    state: "Planned",
    title: "North Star grammar closure",
    description:
      "Bitwise, nested-type, enum, and remaining grammar slices build on the completed M18 foundation.",
  },
];

const leadershipOutcomes = [
  {
    label: "Cycle time",
    value: "Minutes → local",
    description:
      "Catch routine compile and test failures before a deployment begins.",
  },
  {
    label: "Environment capacity",
    value: "Org-light",
    description:
      "Reserve scarce Salesforce environments for compatibility-sensitive validation.",
  },
  {
    label: "CI reliability",
    value: "Deterministic",
    description:
      "Isolated execution, stable ordering, and explicit unsupported behavior.",
  },
];

export default function Home() {
  return (
    <main>
      <nav className="site-nav" aria-label="Primary navigation">
        <a className="wordmark" href="#top" aria-label="Apex Exec home">
          <span className="wordmark-mark" aria-hidden="true">
            AX
          </span>
          <span>Apex Exec</span>
        </a>
        <div className="nav-links">
          <a href="#case">Why now</a>
          <a href="#evidence">Evidence</a>
          <a href="#roadmap">Roadmap</a>
        </div>
        <a
          className="nav-cta"
          href="https://github.com/a-barwick/apex-exec"
          target="_blank"
          rel="noreferrer"
        >
          View repository
          <span aria-hidden="true">↗</span>
        </a>
      </nav>

      <section className="hero" id="top">
        <div className="hero-copy">
          <div className="eyebrow">
            <span className="status-dot" aria-hidden="true" />
            S0 complete · Phase 2 stabilized
          </div>
          <h1>
            Move the Apex
            <br />
            inner loop <em>off the org.</em>
          </h1>
          <p className="hero-lede">
            Apex Exec is a deterministic, org-independent compiler and runtime
            built to give Salesforce engineering teams fast compile, test, and
            debug feedback on developer machines and ordinary CI workers.
          </p>
          <div className="hero-actions">
            <a className="button button-primary" href="#case">
              Review the engineering case
              <span aria-hidden="true">↓</span>
            </a>
            <a
              className="button button-secondary"
              href="https://github.com/a-barwick/apex-exec/blob/main/ROADMAP.md"
              target="_blank"
              rel="noreferrer"
            >
              Read the roadmap
              <span aria-hidden="true">↗</span>
            </a>
          </div>
        </div>

        <div className="hero-visual" aria-label="Apex Exec command line example">
          <div className="terminal">
            <div className="terminal-bar">
              <div className="terminal-dots" aria-hidden="true">
                <span />
                <span />
                <span />
              </div>
              <span>local-ci / apex-exec</span>
              <span className="terminal-live">ready</span>
            </div>
            <div className="terminal-body">
              <div className="terminal-command">
                <span className="prompt">$</span>
                <span>apex-exec test force-app --jobs 4</span>
              </div>
              <div className="terminal-rule" />
              <div className="terminal-line">
                <span className="pass">PASS</span>
                <span>CalculatorTest.addsPositiveValues</span>
              </div>
              <div className="terminal-line">
                <span className="pass">PASS</span>
                <span>CalculatorTest.handlesNegativeValues</span>
              </div>
              <div className="coverage-block">
                <div className="coverage-header">
                  <span>Production coverage</span>
                  <strong>100%</strong>
                </div>
                <div className="coverage-track">
                  <span />
                </div>
                <div className="coverage-meta">
                  <span>3/3 lines</span>
                  <span>2/2 branches</span>
                </div>
              </div>
              <div className="terminal-summary">
                <span>2 passed</span>
                <span>0 failed</span>
                <span>no org connection</span>
              </div>
            </div>
          </div>
          <div className="visual-note">
            <span>Current proof point</span>
            <strong>Useful Apex tests already run locally.</strong>
          </div>
        </div>
      </section>

      <section className="signal-strip" aria-label="Project signals">
        <div>
          <strong>17</strong>
          <span>milestones complete</span>
        </div>
        <div>
          <strong>14,740</strong>
          <span>real-world Apex lines pinned</span>
        </div>
        <div>
          <strong>Rust</strong>
          <span>deterministic runtime core</span>
        </div>
        <div>
          <strong>Explicit</strong>
          <span>compatibility contract</span>
        </div>
      </section>

      <section className="leadership-case section" id="case">
        <div className="section-intro">
          <p className="section-kicker">The leadership case</p>
          <h2>
            The org should be the final oracle.
            <br />
            <span>Not the everyday feedback loop.</span>
          </h2>
          <p>
            Salesforce teams spend high-value engineering time waiting on
            environments to answer questions a local toolchain should resolve.
            Apex Exec shifts routine validation left while keeping final
            platform verification honest.
          </p>
        </div>

        <div className="outcomes-grid">
          {leadershipOutcomes.map((outcome) => (
            <article className="outcome-card" key={outcome.label}>
              <p>{outcome.label}</p>
              <h3>{outcome.value}</h3>
              <span>{outcome.description}</span>
            </article>
          ))}
        </div>

        <div className="operating-model">
          <div className="model-label">Target operating model</div>
          <div className="model-flow" role="list">
            <div className="model-step" role="listitem">
              <span>01</span>
              <strong>Edit</strong>
              <p>Developer workstation</p>
            </div>
            <div className="flow-arrow" aria-hidden="true">
              →
            </div>
            <div className="model-step model-step-highlight" role="listitem">
              <span>02</span>
              <strong>Compile + test</strong>
              <p>Apex Exec locally and in CI</p>
            </div>
            <div className="flow-arrow" aria-hidden="true">
              →
            </div>
            <div className="model-step" role="listitem">
              <span>03</span>
              <strong>Verify + deploy</strong>
              <p>Targeted Salesforce gate</p>
            </div>
          </div>
        </div>
      </section>

      <section className="evidence-section section" id="evidence">
        <div className="evidence-heading">
          <div>
            <p className="section-kicker section-kicker-dark">
              What exists today
            </p>
            <h2>Built as infrastructure, not a demo.</h2>
          </div>
          <p>
            Seventeen completed milestones form a coherent local-development
            and evidence foundation. Each layer has clear ownership, executable
            coverage, and deliberately bounded compatibility claims.
          </p>
        </div>

        <div className="capabilities">
          {shippedCapabilities.map((capability) => (
            <article className="capability" key={capability.number}>
              <div className="capability-number">{capability.number}</div>
              <div>
                <h3>{capability.title}</h3>
                <p>{capability.description}</p>
                <span>{capability.proof}</span>
              </div>
            </article>
          ))}
        </div>

        <div className="honesty-panel">
          <div className="honesty-title">
            <span>Compatibility posture</span>
            <strong>Measured, never implied.</strong>
          </div>
          <p>
            Supported behavior is classified as Compatible or Simplified.
            Unsupported syntax and platform behavior are rejected explicitly.
            Nothing is labeled Exact until differential Salesforce fixtures
            prove it.
          </p>
          <a
            href="https://github.com/a-barwick/apex-exec/blob/main/docs/COMPATIBILITY.md"
            target="_blank"
            rel="noreferrer"
          >
            Inspect the compatibility contract
            <span aria-hidden="true">↗</span>
          </a>
        </div>
      </section>

      <section className="roadmap-section section" id="roadmap">
        <div className="roadmap-heading">
          <div>
            <p className="section-kicker">Execution path</p>
            <h2>From useful today to enterprise leverage.</h2>
          </div>
          <p>
            The decisive product threshold is not complete emulation. It is
            running 60–80% of an enterprise project&apos;s ordinary Apex tests
            locally, quickly, and without source changes.
          </p>
        </div>

        <div className="roadmap-list">
          {roadmap.map((item) => (
            <article className="roadmap-item" key={item.milestone}>
              <div className="roadmap-meta">
                <strong>{item.milestone}</strong>
                <span className={`state state-${item.state.toLowerCase()}`}>
                  {item.state}
                </span>
              </div>
              <h3>{item.title}</h3>
              <p>{item.description}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="decision-section">
        <div className="decision-card">
          <div className="decision-copy">
            <p className="section-kicker section-kicker-dark">
              A candid investment thesis
            </p>
            <h2>
              Reduce dependence.
              <br />
              Preserve confidence.
            </h2>
            <p>
              Apex Exec has a working local feedback loop and reviewed live
              validation evidence. Bounded stabilization and M18 null-aware
              expressions are complete; M19 is next.
            </p>
          </div>
          <div className="decision-actions">
            <a
              className="button button-light"
              href="https://github.com/a-barwick/apex-exec"
              target="_blank"
              rel="noreferrer"
            >
              Evaluate the repository
              <span aria-hidden="true">↗</span>
            </a>
            <a
              className="text-link"
              href="https://github.com/a-barwick/apex-exec/blob/main/docs/VISION.md"
              target="_blank"
              rel="noreferrer"
            >
              Read the product vision
              <span aria-hidden="true">→</span>
            </a>
          </div>
        </div>
      </section>

      <footer>
        <a className="wordmark wordmark-footer" href="#top">
          <span className="wordmark-mark" aria-hidden="true">
            AX
          </span>
          <span>Apex Exec</span>
        </a>
        <p>
          Local-first Apex development.
          <br />
          Salesforce remains the final compatibility oracle.
        </p>
        <div className="footer-links">
          <a
            href="https://github.com/a-barwick/apex-exec"
            target="_blank"
            rel="noreferrer"
          >
            GitHub ↗
          </a>
          <a
            href="https://github.com/a-barwick/apex-exec/blob/main/docs/ARCHITECTURE.md"
            target="_blank"
            rel="noreferrer"
          >
            Architecture ↗
          </a>
          <a
            href="https://github.com/a-barwick/apex-exec/blob/main/docs/STATUS.md"
            target="_blank"
            rel="noreferrer"
          >
            Status ↗
          </a>
        </div>
      </footer>
    </main>
  );
}
