export type Shell = 'powershell' | 'bash'

interface ShellToggleProps {
  selected: Shell
  onChange: (shell: Shell) => void
}

const TERMINAL_ICON = (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    width="24"
    height="24"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    className="size-6 text-nord-4/60"
  >
    <path d="M5 7l5 5l-5 5" />
    <path d="M12 19l7 0" />
  </svg>
)

const SHELLS: { id: Shell; label: string }[] = [
  { id: 'bash', label: 'bash' },
  { id: 'powershell', label: 'pwsh' },
]

export function ShellToggle({ selected, onChange }: ShellToggleProps) {
  return (
    <div className="flex items-center gap-1.5" role="tablist" aria-orientation="horizontal">
      <div className="flex size-4 items-center justify-center rounded-[1px] opacity-60">
        {TERMINAL_ICON}
      </div>
      {SHELLS.map((shell) => (
        <button
          key={shell.id}
          role="tab"
          aria-selected={selected === shell.id}
          onClick={() => onChange(shell.id)}
          className={`
            inline-flex items-center justify-center rounded-xl px-1.5 py-0.5
            text-xs font-medium whitespace-nowrap transition-all
            ${
              selected === shell.id
                ? 'bg-nord-2/80 text-nord-6 shadow-sm'
                : 'text-nord-4/60 hover:text-nord-6/70 hover:bg-nord-2/30'
            }
          `}
        >
          {shell.label}
        </button>
      ))}
    </div>
  )
}
