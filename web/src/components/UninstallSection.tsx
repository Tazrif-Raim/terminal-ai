import { CopyButton } from "@/components/CopyButton"

const UNINSTALL_CMD = "irm https://terminal-ai.lab-node.me/uninstall.ps1 | iex"

export function UninstallSection() {
  return (
    <section className="py-12 md:py-20">
      {/* Section header */}
      <h2 className="mb-8 font-mono text-lg text-nord-6 md:text-xl">
        <span className="text-nord-3">$</span> <span className="text-nord-5"># Uninstall</span>
      </h2>

      {/* Uninstall box */}
      <div className="rounded border border-nord-2 bg-nord-1 overflow-hidden">
        <div className="flex items-center gap-3 px-4 py-3">
          <code className="flex-1 break-all font-mono text-sm text-nord-10">
            {UNINSTALL_CMD}
          </code>
          <CopyButton text={UNINSTALL_CMD} />
        </div>
      </div>

      <p className="mt-3 font-mono text-xs text-nord-3">
        Removes Terminal AI and associated files cleanly.
      </p>
    </section>
  )
}
