import { useState } from "react";
import { Sidebar, type View } from "./components/Sidebar";
import { LibraryView } from "./views/LibraryView";
import { SetlistsView } from "./views/SetlistsView";
import { SetlistEditor } from "./views/SetlistEditor";
import { TransitionsView } from "./views/TransitionsView";
import { ProfileView } from "./views/ProfileView";

function App() {
  const [view, setView] = useState<View>("library");
  const [openSetlistId, setOpenSetlistId] = useState<string | null>(null);

  function changeView(v: View) {
    setOpenSetlistId(null);
    setView(v);
  }

  return (
    <div className="flex h-screen w-screen overflow-hidden">
      <Sidebar view={view} onChange={changeView} />
      <main className="flex-1 overflow-hidden">
        {view === "library" && <LibraryView />}
        {view === "setlists" &&
          (openSetlistId ? (
            <SetlistEditor setlistId={openSetlistId} onBack={() => setOpenSetlistId(null)} />
          ) : (
            <SetlistsView onOpen={setOpenSetlistId} />
          ))}
        {view === "transitions" && <TransitionsView />}
        {view === "profile" && <ProfileView />}
      </main>
    </div>
  );
}

export default App;
