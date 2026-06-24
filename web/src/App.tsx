import { useState } from 'react'
import { NavBar } from "@/components/NavBar"
import { HeroSection } from "@/components/HeroSection"
import { InstallSection } from "@/components/InstallSection"
import { HowItWorks } from "@/components/HowItWorks"
import { ApiSetup } from "@/components/ApiSetup"
import { UninstallSection } from "@/components/UninstallSection"
import { Footer } from "@/components/Footer"
import type { Shell } from "@/components/ShellToggle"

function getDefaultShell(): Shell {
  if (typeof navigator !== 'undefined' && navigator.userAgent.toLowerCase().includes('win')) {
    return 'powershell'
  }
  return 'bash'
}

function App() {
  const [selectedShell, setSelectedShell] = useState<Shell>(getDefaultShell)

  return (
    <div className="scanline grid-bg min-h-svh">
      <NavBar />
      <main className="mx-auto max-w-5xl px-4 py-8 md:py-12">
        <HeroSection />

        <div className="section-divider">─────────────────────</div>

        <InstallSection selectedShell={selectedShell} onShellChange={setSelectedShell} />

        <div className="section-divider">─────────────────────</div>

        <HowItWorks />

        <div className="section-divider">─────────────────────</div>

        <ApiSetup />

        <div className="section-divider">─────────────────────</div>

        <UninstallSection selectedShell={selectedShell} onShellChange={setSelectedShell} />
      </main>
      <Footer />
    </div>
  )
}

export default App
