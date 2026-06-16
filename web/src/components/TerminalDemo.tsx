import { useState, useEffect, useRef } from "react"

/* ------------------------------------------------------------------ */
/*  Frames — each frame is an array of lines with optional styling    */
/* ------------------------------------------------------------------ */

interface FrameLine {
  text: string
  color?: string // tailwind text color class
}

const FRAMES: FrameLine[][] = [
  // ── Frame 1: Simple query ──
  [
    { text: "> ai what is running on port 3000", color: "text-nord-6" },
    { text: "" },
    { text: "[thinking...]", color: "text-nord-3" },
  ],

  // ── Frame 2: Command suggestion UI ──
  [
    { text: "Select command", color: "text-nord-5 font-bold" },
    { text: "" },
    { text: "  Get-NetTCPConnection -LocalPort 3000 | Select-Object ...  [safe]", color: "text-nord-3" },
    { text: "❯ netstat -ano | findstr :3000   [safe]", color: "text-nord-8" },
    { text: "" },
    { text: "Up/Down or w/s = select | Enter = run | e = edit | c = copy", color: "text-nord-2 text-xs" },
  ],

  // ── Frame 3: Agent mode start ──
  [
    { text: "> ai --agent setup a new React project with TypeScript", color: "text-nord-6" },
    { text: "" },
    { text: "agent mode starting", color: "text-nord-9" },
    { text: "─────────────────────────────────────", color: "text-nord-2" },
    { text: "Step 1 / ~5  |  RunCommand  |  [Safe]", color: "text-nord-7" },
    { text: "─────────────────────────────────────", color: "text-nord-2" },
    { text: "" },
    { text: "CWD : C:\\Users\\dev\\projects", color: "text-nord-4/70" },
    { text: "CMD : npx create-react-app my-app --template typescript", color: "text-nord-4" },
    { text: "WHY : Scaffold a new React project with TypeScript template", color: "text-nord-6" },
  ],
]

/* ------------------------------------------------------------------ */
/*  TerminalDemo component                                            */
/* ------------------------------------------------------------------ */

export function TerminalDemo() {
  const [frameIdx, setFrameIdx] = useState(0)
  const [visibleLines, setVisibleLines] = useState<number>(0)
  const [typedChars, setTypedChars] = useState<Record<number, number>>({})
  const [phase, setPhase] = useState<"typing" | "pause" | "clearing">("typing")
  const containerRef = useRef<HTMLDivElement>(null)

  const currentFrame = FRAMES[frameIdx]

  // Reset animation whenever frame changes
  useEffect(() => {
    setVisibleLines(0)
    setTypedChars({})
    setPhase("typing")
  }, [frameIdx])

  // Typing engine
  useEffect(() => {
    if (phase !== "typing") return

    const lineIdx = visibleLines
    if (lineIdx >= currentFrame.length) {
      // All lines shown — pause, then advance
      setPhase("pause")
      return
    }

    const line = currentFrame[lineIdx]
    const fullText = line.text
    const alreadyTyped = typedChars[lineIdx] ?? 0

    if (fullText === "") {
      // Empty line, show immediately and advance
      setTypedChars((p) => ({ ...p, [lineIdx]: 0 }))
      setVisibleLines((p) => p + 1)
      return
    }

    if (alreadyTyped < fullText.length) {
      const chunk = Math.min(3, fullText.length - alreadyTyped)
      const speed = lineIdx === 0 ? 20 : 15 + Math.random() * 20
      const timer = setTimeout(() => {
        setTypedChars((p) => ({ ...p, [lineIdx]: alreadyTyped + chunk }))
      }, speed)
      return () => clearTimeout(timer)
    }

    // Done with this line, wait a bit then advance
    const pause = 120 + Math.random() * 100
    const timer = setTimeout(() => {
      setVisibleLines((p) => p + 1)
    }, pause)
    return () => clearTimeout(timer)
  }, [phase, visibleLines, typedChars, currentFrame])

  // Pause phase — wait, then move to next frame
  useEffect(() => {
    if (phase !== "pause") return
    const timer = setTimeout(() => {
      setFrameIdx((p) => (p + 1) % FRAMES.length)
    }, 3000)
    return () => clearTimeout(timer)
  }, [phase])

  /* ---- render ---- */

  return (
    <div
      ref={containerRef}
      className="terminal-window mx-auto w-full max-w-2xl"
    >
      {/* Window title bar */}
      <div className="flex items-center gap-1.5 border-b border-nord-2 bg-nord-1 px-3 py-2">
        <span className="size-2.5 rounded-full bg-nord-10" />
        <span className="size-2.5 rounded-full bg-nord-9" />
        <span className="size-2.5 rounded-full bg-nord-8" />
        <span className="ml-2 text-[10px] text-nord-3 tracking-wide">
          terminal-ai — bash
        </span>
      </div>

      {/* Terminal content */}
      <div className="min-h-[260px] bg-nord-0 p-4 font-mono text-sm leading-relaxed">
        {currentFrame.map((line, li) => {
          if (li > visibleLines) return null
          const typed = typedChars[li] ?? line.text.length
          const shown = line.text === "" ? "" : line.text.slice(0, typed)

          return (
            <div key={li} className={`${line.color ?? "text-nord-4"} ${li === 0 ? "animate-fade-in" : ""}`}>
              <span>{shown}</span>
              {li === visibleLines && li === currentFrame.length - 1 && (
                <span className="inline-block h-4 w-2 bg-nord-6 ml-0.5 align-middle animate-blink" />
              )}
              {li === visibleLines && li < currentFrame.length - 1 && (
                <span className="inline-block h-4 w-2 bg-nord-6 ml-0.5 align-middle animate-blink" />
              )}
            </div>
          )
        })}

        {/* Empty state: cursor before any text typed */}
        {visibleLines === 0 && Object.keys(typedChars).length === 0 && (
          <span className="inline-block h-4 w-2 bg-nord-6 animate-blink" />
        )}
      </div>
    </div>
  )
}
