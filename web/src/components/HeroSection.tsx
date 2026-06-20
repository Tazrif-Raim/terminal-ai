import { Zap, LogIn, Shield } from "lucide-react"
import { TerminalDemo } from "@/components/TerminalDemo"

export function HeroSection() {
  return (
    <section className="flex flex-col items-center py-12 text-center md:py-16">
      {/* ASCII-style heading */}
      <h1 className="glow-cyan mb-2 text-5xl font-bold tracking-[0.15em] text-nord-4 md:text-7xl">
        TERMINAL
        <br />
        <span className="text-nord-6">AI</span>
      </h1>

      {/* Subtitle */}
      <p className="mt-4 max-w-xl text-base text-nord-4 md:text-lg">
        Natural language → Correct terminal command <span className="text-nord-8">Instantly.</span>
      </p>

      {/* Feature badges */}
      <div className="mt-4 flex flex-wrap items-center justify-center gap-4 max-w-2xl">
        <span className="inline-flex items-center gap-1.5 rounded-full border border-nord-2 bg-nord-1/60 px-3 py-1 font-mono text-xs text-nord-6">
          <Zap className="size-3.5" />
          Powered by OpenAI-compatible API
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-nord-2 bg-nord-1/60 px-3 py-1 font-mono text-xs text-nord-9">
          <LogIn className="size-3.5" />
          Sign in with ChatGPT/Codex OAuth
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-nord-2 bg-nord-1/60 px-3 py-1 font-mono text-xs text-nord-8">
          <Shield className="size-3.5" />
          Agent mode with safety guardrails
        </span>
      </div>

      {/* Terminal demo animation */}
      <div className="mt-10 w-full px-2 text-left">
        <TerminalDemo />
      </div>
    </section>
  )
}
