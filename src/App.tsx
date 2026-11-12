import { useEffect, useState } from "react";
import { Sidebar, type View } from "./components/Sidebar";
import { OnboardingTutorial } from "./components/OnboardingTutorial";
import { GlobalAnalyzeProgress } from "./components/GlobalAnalyzeProgress";
import { MiniPlayer } from "./components/MiniPlayer";
import { usePlayback } from "./store/playback";
import { LibraryView } from "./views/LibraryView";
import { SetlistsView } from "./views/SetlistsView";
import { SetlistEditor } from "./views/SetlistEditor";
import { ProfileView } from "./views/ProfileView";

const ONBOARDING_SEEN_KEY = "gigflow.onboarding_seen";

function App() {
  const [view, setView] = useState<View>("library");
  const [openSetlistId, setOpenSetlistId] = useState<string | null>(null);
  const [showOnboarding, setShowOnboarding] = useState(false);
  const nowPlaying = usePlayback((s) => s.decks.length > 0);
  const togglePause = usePlayback((s) => s.togglePause);

  useEffect(() => {
    try {
      if (!localStorage.getItem(ONBOARDING_SEEN_KEY)) setShowOnboarding(true);
    } catch {
      // localStorage unavailable (private window, etc.) — just skip auto-show.
    }
  }, []);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.code !== "Space" || !nowPlaying) return;
      const target = e.target as HTMLElement | null;
      const tag = target?.tagName;
      // Don't hijack space while the user is typing into a field — only act
      // when nothing text-editable has focus.
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
      e.preventDefault();
      togglePause();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [nowPlaying, togglePause]);

  function closeOnboarding() {
    setShowOnboarding(false);
    try {
      localStorage.setItem(ONBOARDING_SEEN_KEY, "1");
    } catch {
      // ignore
    }
  }

  function changeView(v: View) {
    setOpenSetlistId(null);
    setView(v);
  }

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden">
      <div className="flex flex-1 min-h-0">
        <Sidebar view={view} onChange={changeView} onHelp={() => setShowOnboarding(true)} />
        <main className="flex-1 overflow-hidden">
          {view === "library" && <LibraryView />}
          {view === "setlists" &&
            (openSetlistId ? (
              <SetlistEditor setlistId={openSetlistId} onBack={() => setOpenSetlistId(null)} />
            ) : (
              <SetlistsView onOpen={setOpenSetlistId} />
            ))}
          {view === "profile" && <ProfileView />}
        </main>
      </div>
      <GlobalAnalyzeProgress />
      <MiniPlayer />
      {showOnboarding && <OnboardingTutorial onClose={closeOnboarding} onChangeView={changeView} />}
    </div>
  );
}

export default App;
