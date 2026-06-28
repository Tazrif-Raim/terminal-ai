# Terminal AI Website

## Tech Stack
- React 19 + Vite 8 + TypeScript 6
- Tailwind CSS v4 (CSS-first config, no tailwind.config.js)
- shadcn/ui (radix-maia style) with Nord theme
- lucide-react for icons
- JetBrains Mono Variable font

## Project Structure
- `web/` - Frontend project root
- `web/src/App.tsx` - Main page with all sections
- `web/src/index.css` - Nord theme CSS variables + terminal effects
- `web/src/components/` - All UI components

## Components
- `NavBar.tsx` - Sticky nav with shell prompt + GitHub link
- `HeroSection.tsx` - Hero with ASCII heading + TerminalDemo
- `TerminalDemo.tsx` - Custom typewriter terminal animation (3 frames cycle)
- `CopyButton.tsx` - Reusable clipboard copy button with feedback
- `InstallSection.tsx` - Install command block with copy (PowerShell/Bash toggle)
- `HowItWorks.tsx` - 3-column steps section
- `ApiSetup.tsx` - BYOK provider cards + config snippet
- `UninstallSection.tsx` - Uninstall command block (PowerShell/Bash toggle)
- `Footer.tsx` - ASCII-styled footer

## Build & Deploy
- Dev: `npm run dev` (from web/)
- Build: `npm run build` (from web/)
- GitHub Pages: `GH_PAGES=true npm run build` to use `/terminal-ai/` base path
- Output: `web/dist/`

## termcn
- `termcn` package not available on npm registry
- Fallback: hand-coded TerminalDemo typewriter component

## Nord Theme
- CSS variables in :root/.dark use Nord palette
- Background: #2E3440, Foreground: #D8DEE9
- Cyan accent: #88C0D0, Green: #A3BE8C, Red: #BF616A
- Terminal effects: scanline overlay, grid background, glow text

## Shell Support
- **Windows**: PowerShell (`.ps1` wrapper, `~\AppData\Roaming\Microsoft\Windows\PowerShell\Microsoft.PowerShell_profile.ps1`)
- **Linux**: Bash (`.bashrc`) and Zsh (`.zshrc`) — installer auto-detects `$SHELL`
- **Install flow**: Installer detects shell and installs appropriate wrapper to appropriate profile
- **Uninstall flow**: Uninstaller cleans both `~/.bashrc` and `~/.zshrc` blocks based on detected shell