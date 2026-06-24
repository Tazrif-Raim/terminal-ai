import { CopyButton } from "@/components/CopyButton"
import { ShellToggle, type Shell } from "@/components/ShellToggle"

interface InstallSectionProps {
  selectedShell: Shell
  onShellChange: (shell: Shell) => void
}

const CMD: Record<Shell, string> = {
  powershell: "irm https://terminal-ai.lab-node.me/powershell.ps1 | iex",
  bash: "curl -fsSL https://terminal-ai.lab-node.me/install.sh | bash",
}

const NOTES: Record<Shell, { paste: string; req: string }> = {
  powershell: {
    paste: "Windows: Paste in PowerShell as Administrator",
    req: "Requires PowerShell",
  },
  bash: {
    paste: "Linux: Paste in Terminal as root",
    req: "Requires curl and bash",
  },
}

export function InstallSection({ selectedShell, onShellChange }: InstallSectionProps) {
  const cmd = CMD[selectedShell]

  return (
    <section className="py-12 md:py-20">
      {/* Section header */}
      <h2 className="mb-8 font-mono text-lg text-nord-6 md:text-xl">
        <span className="text-nord-3">$</span> <span className="text-nord-5"># Get Terminal AI</span>
      </h2>

      {/* Install box */}
      <div className="rounded border border-nord-2 bg-nord-1 overflow-hidden">
        {/* Title bar */}
        <div className="border-b border-nord-2 bg-nord-0/50 px-4 py-1.5 text-xs text-nord-3 flex items-center">
          <ShellToggle selected={selectedShell} onChange={onShellChange} />
        </div>

        {/* Command */}
        <div className="flex items-center gap-3 px-4 py-3">
          <code className="flex-1 break-all font-mono text-sm text-nord-8">
            {cmd}
          </code>
          <CopyButton text={cmd} />
        </div>
      </div>

      {/* Notes */}
      <div className="mt-3 space-y-1">
        <p className="font-mono text-[11px] text-nord-3">
          ⚠ {NOTES[selectedShell].paste}
        </p>
        <p className="font-mono text-[11px] text-nord-3">
          ⚠ {NOTES[selectedShell].req}
        </p>
      </div>
    </section>
  )
}
