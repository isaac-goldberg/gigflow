export type View = "library" | "setlists" | "transitions" | "profile";

const ITEMS: { id: View; label: string }[] = [
  { id: "library", label: "Library" },
  { id: "setlists", label: "Setlists" },
  { id: "transitions", label: "Transitions" },
  { id: "profile", label: "DJ Profile" },
];

export function Sidebar({ view, onChange }: { view: View; onChange: (v: View) => void }) {
  return (
    <aside className="w-48 bg-gig-panel border-r border-gig-border flex flex-col shrink-0">
      <div className="h-14 flex items-center px-4 border-b border-gig-border">
        <div className="text-base font-black tracking-wider uppercase">
          <span className="text-gig-accent">GIG</span>
          <span className="text-gig-text">FLOW</span>
        </div>
      </div>
      <nav className="flex-1 py-2">
        {ITEMS.map((item) => {
          const active = view === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onChange(item.id)}
              className={`w-full flex items-center gap-2.5 pl-4 pr-3 py-2.5 text-[11px] font-bold uppercase tracking-wider border-l-2 ${
                active
                  ? "bg-gig-panel3 text-gig-accent border-gig-accent"
                  : "text-gig-muted border-transparent hover:bg-gig-panel2 hover:text-gig-text"
              }`}
            >
              <span
                className="w-1.5 h-1.5 rounded-full shrink-0"
                style={{ backgroundColor: active ? "#9bff1f" : "#3d3d41" }}
              />
              {item.label}
            </button>
          );
        })}
      </nav>
      <div className="px-4 py-3 border-t border-gig-border label-xs">Gigflow · v0.1.0</div>
    </aside>
  );
}
