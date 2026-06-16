import { CopyButton } from "@/components/CopyButton"

const INSTALL_CMD = "irm https://terminal-ai.lab-node.me/powershell.ps1 | iex"

export function InstallSection() {
  return (
    <section className="py-12 md:py-20">
      {/* Section header */}
      <h2 className="mb-8 font-mono text-lg text-nord-6 md:text-xl">
        <span className="text-nord-3">$</span> <span className="text-nord-5"># Get Terminal AI</span>
      </h2>

      {/* Install box */}
      <div className="rounded border border-nord-2 bg-nord-1 overflow-hidden">
        {/* Title bar */}
        <div className="border-b border-nord-2 bg-nord-0/50 px-4 py-2 text-xs text-nord-3">
          PowerShell (Windows)
        </div>

        {/* Command */}
        <div className="flex items-center gap-3 px-4 py-3">
          <code className="flex-1 break-all font-mono text-sm text-nord-8">
            {INSTALL_CMD}
          </code>
          <CopyButton text={INSTALL_CMD} />
        </div>
      </div>

      {/* Bash — coming soon */}
      <p className="mt-3 font-mono text-xs text-nord-3 italic">
        Bash — coming soon{" "}
        <span className="not-italic">🔜</span>
      </p>

      {/* Note */}
      <p className="mt-2 font-mono text-[11px] text-nord-3">
        ⚠ Paste in <strong className="text-nord-9">PowerShell as Administrator</strong>
      </p>
    </section>
  )
}
