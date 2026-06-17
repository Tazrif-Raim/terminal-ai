import { NavBar } from "@/components/NavBar"
import { HeroSection } from "@/components/HeroSection"
import { InstallSection } from "@/components/InstallSection"
import { HowItWorks } from "@/components/HowItWorks"
import { ApiSetup } from "@/components/ApiSetup"
import { UninstallSection } from "@/components/UninstallSection"
import { Footer } from "@/components/Footer"

function App() {
  return (
    <div className="scanline grid-bg min-h-svh">
      <NavBar />
      <main className="mx-auto max-w-5xl px-4 py-8 md:py-12">
        <HeroSection />

        <div className="section-divider">─────────────────────</div>

        <InstallSection />

        <div className="section-divider">─────────────────────</div>

        <HowItWorks />

        <div className="section-divider">─────────────────────</div>

        <ApiSetup />

        <div className="section-divider">─────────────────────</div>

        <UninstallSection />
      </main>
      <Footer />
    </div>
  )
}

export default App
