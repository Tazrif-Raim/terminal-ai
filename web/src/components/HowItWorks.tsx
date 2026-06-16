export function HowItWorks() {
  const steps = [
    {
      title: "Step 1 — Ask",
      code: `> ai what is running on port 5432`,
      desc: "Type your goal in plain English",
      accent: "text-nord-6",
    },
    {
      title: "Step 2 — AI suggests",
      code: `→ netstat -ano | findstr :5432  [safe]`,
      desc: "Terminal AI translates it to the exact command",
      accent: "text-nord-8",
    },
    {
      title: "Step 3 — Agent mode",
      code: `> ai --agent clone this repo and set it up for local dev
agent mode starting
Step 1/4 | git clone ...   | [Safe]
Step 2/4 | npm install     | [Safe]`,
      desc: "Agent mode autonomously executes multi-step tasks with safety checks at every step",
      accent: "text-nord-9",
    },
  ]

  return (
    <section className="py-12 md:py-20">
      {/* Section header */}
      <h2 className="mb-8 font-mono text-lg text-nord-6 md:text-xl">
        <span className="text-nord-3">$</span> <span className="text-nord-5"># How it works</span>
      </h2>

      {/* Steps — grid on desktop, stacked on mobile */}
      <div className="grid gap-6 md:grid-cols-3">
        {steps.map((step, i) => (
          <div
            key={i}
            className="flex flex-col rounded border border-nord-2 bg-nord-1/50 p-4"
          >
            <h3 className={`mb-3 font-mono text-sm font-bold ${step.accent}`}>
              {step.title}
            </h3>

            {/* Terminal code block */}
            <div className="mb-3 flex-1 rounded border border-nord-2 bg-nord-0 p-3">
              <pre className="overflow-x-auto font-mono text-xs leading-relaxed text-nord-4 whitespace-pre-wrap">
                {step.code}
              </pre>
            </div>

            <p className="font-mono text-xs text-nord-3">{step.desc}</p>
          </div>
        ))}
      </div>
    </section>
  )
}
