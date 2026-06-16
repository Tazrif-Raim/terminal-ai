export function ApiSetup() {
  const providers = [
    {
      name: "Google Gemini",
      note: "Free tier available",
      href: "https://aistudio.google.com/apikey",
      color: "text-nord-7",
    },
    {
      name: "Groq",
      note: "Ultra-fast inference, free tier",
      href: "https://console.groq.com/keys",
      color: "text-nord-8",
    },
    {
      name: "OpenAI",
      note: "GPT-4o and beyond",
      href: "https://platform.openai.com/api-keys",
      color: "text-nord-6",
    },
  ]

  return (
    <section className="py-12 md:py-20">
      {/* Section header */}
      <h2 className="mb-4 font-mono text-lg text-nord-6 md:text-xl">
        <span className="text-nord-3">$</span> <span className="text-nord-5"># Bring Your Own Key</span>
      </h2>

      <p className="mb-6 max-w-2xl font-mono text-sm text-nord-3">
        Terminal AI uses the OpenAI-compatible API format. Use any provider — free or paid.
      </p>

      {/* Provider cards */}
      <div className="grid gap-4 md:grid-cols-3">
        {providers.map((p) => (
          <a
            key={p.name}
            href={p.href}
            target="_blank"
            rel="noopener noreferrer"
            className="group block rounded border border-nord-2 bg-nord-1/50 p-4 no-underline transition-all hover:border-nord-6/40 hover:bg-nord-1"
          >
            <div className="font-mono text-sm font-bold text-nord-4 group-hover:text-nord-5">
              [{p.name}]
            </div>
            <div className="mt-1 font-mono text-xs text-nord-3">{p.note}</div>
            <div className={`mt-2 font-mono text-[11px] ${p.color} underline-offset-2 group-hover:underline`}>
              Get API key →
            </div>
          </a>
        ))}
      </div>

      {/* Config snippet */}
      <div className="mt-8 rounded border border-nord-2 bg-nord-1 overflow-hidden">
        <div className="border-b border-nord-2 bg-nord-0/50 px-4 py-2 text-xs text-nord-3">
          Terminal AI configuration
        </div>
        <pre className="overflow-x-auto p-4 font-mono text-xs leading-relaxed text-nord-4 whitespace-pre-wrap">
{`> ai --config
> ? Enter your API base URL: https://api.openai.com/v1/chat/completions
> ? Enter your API key: ••••••••••••
> ? Enter your preferred model: gpt-5
> ✓ Config saved.`}
        </pre>
      </div>
    </section>
  )
}
