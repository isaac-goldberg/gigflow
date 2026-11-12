import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Profile } from "../types";

const emptyProfile: Profile = {
  id: "local",
  display_name: "",
  bio: "",
  socials: [],
  work_links: [],
  avatar_path: null,
};

export function ProfileView() {
  const [profile, setProfile] = useState<Profile>(emptyProfile);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    api.getProfile().then(setProfile);
  }, []);

  async function save() {
    const updated = await api.updateProfile(profile);
    setProfile(updated);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  function updateSocial(i: number, field: "platform" | "url", value: string) {
    const next = [...profile.socials];
    next[i] = { ...next[i], [field]: value };
    setProfile({ ...profile, socials: next });
  }

  function updateWork(i: number, field: "title" | "url", value: string) {
    const next = [...profile.work_links];
    next[i] = { ...next[i], [field]: value };
    setProfile({ ...profile, work_links: next });
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between h-14 px-5 border-b border-gig-border bg-gig-panel">
        <div className="flex items-baseline gap-3">
          <h1 className="text-sm font-bold uppercase tracking-widest text-gig-text">DJ Profile</h1>
          <span className="label-xs">Your bio, socials and work — shown when you share with other DJs</span>
        </div>
        <button onClick={save} className="px-3 py-1.5 rounded-sm text-xs font-bold uppercase tracking-wide bg-gig-accent text-black hover:bg-gig-accent/80">
          {saved ? "Saved ✓" : "Save"}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto px-5 py-5 max-w-2xl space-y-5">
        <div className="border border-dashed border-gig-border px-3 py-2 text-xs text-gig-muted">
          Profile data is stored locally for now. Publishing a public profile and sharing with other
          Gigflow users needs cloud accounts — coming in a future update.
        </div>

        <div>
          <label className="label-xs">DJ / artist name</label>
          <input
            value={profile.display_name}
            onChange={(e) => setProfile({ ...profile, display_name: e.target.value })}
            placeholder="DJ Nova"
            className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1.5 text-sm mt-1"
          />
        </div>

        <div>
          <label className="label-xs">Bio</label>
          <textarea
            value={profile.bio}
            onChange={(e) => setProfile({ ...profile, bio: e.target.value })}
            placeholder="Tell other DJs about your sound, residencies, favorite genres…"
            rows={4}
            className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1.5 text-sm mt-1 resize-none"
          />
        </div>

        <div>
          <div className="flex items-center justify-between">
            <label className="label-xs">Social links</label>
            <button
              onClick={() => setProfile({ ...profile, socials: [...profile.socials, { platform: "", url: "" }] })}
              className="text-[11px] font-bold uppercase text-gig-accent2 hover:underline"
            >
              + Add link
            </button>
          </div>
          <div className="space-y-2 mt-1">
            {profile.socials.map((s, i) => (
              <div key={i} className="flex gap-2">
                <input
                  value={s.platform}
                  onChange={(e) => updateSocial(i, "platform", e.target.value)}
                  placeholder="Instagram"
                  className="w-32 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm"
                />
                <input
                  value={s.url}
                  onChange={(e) => updateSocial(i, "url", e.target.value)}
                  placeholder="https://instagram.com/..."
                  className="flex-1 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm"
                />
                <button
                  onClick={() => setProfile({ ...profile, socials: profile.socials.filter((_, j) => j !== i) })}
                  className="text-gig-muted hover:text-gig-warn px-2"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        </div>

        <div>
          <div className="flex items-center justify-between">
            <label className="label-xs">My work (mixes, releases, links)</label>
            <button
              onClick={() => setProfile({ ...profile, work_links: [...profile.work_links, { title: "", url: "" }] })}
              className="text-[11px] font-bold uppercase text-gig-accent2 hover:underline"
            >
              + Add work
            </button>
          </div>
          <div className="space-y-2 mt-1">
            {profile.work_links.map((w, i) => (
              <div key={i} className="flex gap-2">
                <input
                  value={w.title}
                  onChange={(e) => updateWork(i, "title", e.target.value)}
                  placeholder="Warehouse Set 2026"
                  className="w-48 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm"
                />
                <input
                  value={w.url}
                  onChange={(e) => updateWork(i, "url", e.target.value)}
                  placeholder="https://soundcloud.com/..."
                  className="flex-1 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm"
                />
                <button
                  onClick={() => setProfile({ ...profile, work_links: profile.work_links.filter((_, j) => j !== i) })}
                  className="text-gig-muted hover:text-gig-warn px-2"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
