import { useState, useEffect } from 'react'
import { NavBar } from "@/components/NavBar"
import { HeroSection } from "@/components/HeroSection"
import { InstallSection } from "@/components/InstallSection"
import { HowItWorks } from "@/components/HowItWorks"
import { ApiSetup } from "@/components/ApiSetup"
import { UninstallSection } from "@/components/UninstallSection"
import { Footer } from "@/components/Footer"
import type { Shell } from "@/components/ShellToggle"

function useDetectedShell(): Shell {
  const [shell, setShell] = useState<Shell>('bash')

  useEffect(() => {
    const ua = navigator.userAgent.toLowerCase()
    if (ua.includes('win')) {
      setShell('powershell')
    } else {
      setShell('bash')
    }
  }, [])

  return shell
}

function App() {
  const detectedShell = useDetectedShell()
  const [selectedShell, setSelectedShell] = useState<Shell>('bash')

  // Sync detected OS once on mount (after detectedShell settles)
  useEffect(() => {
    setSelectedShell(detectedShell)
  }, [detectedShell])

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
