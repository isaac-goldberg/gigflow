import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { HotCue } from "../types";
import { TRANSITION_TYPES } from "../types";
import { useEscapeKey } from "../lib/useEscapeKey";
import { formatClock, padLetter } from "./HotCueStrip";
import { barsToSecs, secsToBars } from "../lib/transitionZone";

const BAR_CHOICES = [1, 2, 4, 8, 16, 32];

/** What each type does, in DJ terms — shown under the type picker. */
const TYPE_HELP: Record<string, string> = {
  crossfade: "EQ blend: next track fades in with its bass cut, basslines swap halfway on a bar line, then this track fades out.",
  "filter sweep": "This track's high-pass filter sweeps up while the next one's low-pass opens, with a bass swap in the middle.",
  cut: "Hard cut on the beat at the mix point.",
  "echo out": "This track echoes out over its last bar; the next one drops in on the downbeat.",
  backspin: "Vinyl backspin on this track, next one drops in when it stops.",
  "loop roll": "This track's first beat loops tighter and tighter (½ → ¼ → ⅛) for a bar, then the next one drops.",
};

/**
 * Configures the transition for one specific gap between two tracks —
 * there's no library of saved/reusable transitions to pick from anymore
 * (a transition's cue points only ever make sense for this exact track
 * pair), so this goes straight to editing rather than listing anything.
 */
export function TransitionPicker({
  entryId,
  trackAId,
  trackBId,
  trackATitle,
  trackBTitle,
  trackABpm,
  initial,
  onSaved,
  onClose,
}: {
  entryId: string;
  trackAId: string;
  trackBId: string;
  trackATitle: string;
  trackBTitle: string;
  trackABpm: number | null;
  initial: {
    transitionType: string | null;
    crossfadeSecs: number | null;
    cueOutSecs: number | null;
    cueInSecs: number | null;
    transitionNotes: string | null;
  };
  onSaved: () => void;
  onClose: () => void;
}) {
  useEscapeKey(onClose);
  const hadExisting = initial.transitionType != null;
  const [type, setType] = useState<string>(initial.transitionType ?? TRANSITION_TYPES[0]);
  const [crossfade, setCrossfade] = useState(initial.crossfadeSecs ?? barsToSecs(8, trackABpm));
  const bars = Math.max(1, Math.round(secsToBars(crossfade, trackABpm)));
  const hasLength = type === "crossfade" || type === "filter sweep";
  const [cueOutSecs, setCueOutSecs] = useState<number | null>(initial.cueOutSecs);
  const [cueInSecs, setCueInSecs] = useState<number | null>(initial.cueInSecs);
  const [notes, setNotes] = useState(initial.transitionNotes ?? "");
  const [cuesA, setCuesA] = useState<HotCue[]>([]);
  const [cuesB, setCuesB] = useState<HotCue[]>([]);
  const [rationale, setRationale] = useState<string | null>(null);
  const [suggesting, setSuggesting] = useState(false);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    api.getHotCues(trackAId).then(setCuesA);
    api.getHotCues(trackBId).then(setCuesB);
  }, [trackAId, trackBId]);

  async function useRecommended() {
    setSuggesting(true);
    try {
      const s = await api.suggestTransition(trackAId, trackBId);
      setType(s.transition_type);
      setCrossfade(s.crossfade_secs);
      setCueOutSecs(s.cue_out_secs);
      setCueInSecs(s.cue_in_secs);
      setRationale(s.rationale);
    } finally {
      setSuggesting(false);
    }
  }

  async function save() {
    setSaving(true);
    try {
      await api.setEntryTransition({
        entryId,
        transitionType: type,
        crossfadeSecs: crossfade,
        cueOutSecs,
        cueInSecs,
        notes: notes || null,
      });
      onSaved();
    } finally {
      setSaving(false);
    }
  }

  async function removeTransition() {
    await api.setEntryTransition({
      entryId,
      transitionType: null,
      crossfadeSecs: null,
      cueOutSecs: null,
      cueInSecs: null,
      notes: null,
    });
    onSaved();
  }

  return (
    // A floating panel, not a full-screen modal: the rest of the page stays
    // visible and interactive behind it (no backdrop), so you can still
    // hover a setlist row's cue points for their tooltips while this is
    // open.
    <div className="fixed top-20 right-4 bottom-4 z-40 w-96 max-w-[calc(100vw-2rem)] bg-gig-panel border border-gig-borderLight rounded-sm shadow-2xl flex flex-col">
      <div className="p-3 border-b border-gig-border bg-gig-panel2 rounded-t-sm">
        <h3 className="text-xs font-bold uppercase tracking-wider">
          {hadExisting ? "Edit transition" : "New transition"}
        </h3>
        <p className="text-[11px] text-gig-muted mt-0.5 truncate">
          {trackATitle} → {trackBTitle}
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        <div>
          <button
            onClick={useRecommended}
            disabled={suggesting}
            className="w-full flex items-center justify-center gap-1.5 bg-gig-accent text-black rounded-sm py-1.5 text-xs font-bold uppercase tracking-wide hover:bg-gig-accent/80 disabled:opacity-60"
          >
            {suggesting && <span className="w-2.5 h-2.5 rounded-full border-[1.5px] border-black/30 border-t-black animate-spin" />}
            {suggesting ? "Calculating…" : "Use recommended"}
          </button>
          {rationale && <p className="text-[10px] text-gig-dim leading-snug mt-1">{rationale}</p>}
        </div>

        <div>
          <div className="flex gap-2">
            <select
              value={type}
              onChange={(e) => setType(e.target.value)}
              className="flex-1 bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm uppercase"
            >
              {TRANSITION_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
            {hasLength && (
              <select
                value={BAR_CHOICES.reduce((best, b) => (Math.abs(b - bars) < Math.abs(best - bars) ? b : best))}
                onChange={(e) => setCrossfade(barsToSecs(parseInt(e.target.value, 10), trackABpm))}
                className="w-28 bg-black border border-gig-border rounded-sm px-2 py-1 text-sm font-mono"
                title="Blend length, in bars of the outgoing track's tempo"
              >
                {BAR_CHOICES.map((b) => (
                  <option key={b} value={b}>
                    {b} bar{b === 1 ? "" : "s"} · {Math.round(barsToSecs(b, trackABpm))}s
                  </option>
                ))}
              </select>
            )}
          </div>
          <p className="text-[10px] text-gig-dim leading-snug mt-1">{TYPE_HELP[type] ?? ""}</p>
        </div>

        <div className="flex gap-2">
          <CuePointSelect
            label={`Start mixing out of "${trackATitle}" at`}
            emptyLabel="Near the end"
            cues={cuesA}
            value={cueOutSecs}
            onChange={setCueOutSecs}
          />
          <CuePointSelect
            label={`Bring "${trackBTitle}" in from`}
            emptyLabel="Start of track"
            cues={cuesB}
            value={cueInSecs}
            onChange={setCueInSecs}
          />
        </div>

        <textarea
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="NOTES: EQ MOVES, EFFECTS…"
          className="w-full bg-black border border-gig-border rounded-sm px-2.5 py-1 text-sm resize-none"
          rows={2}
        />

        <p className="text-[10px] text-gig-dim leading-snug">
          To hear it, save and press play on the first track — the next one is beatmatched and mixed in live at the
          marked zone. Seek just before the zone to jump straight to the transition.
        </p>
      </div>

      <div className="p-2.5 border-t border-gig-border bg-gig-panel2 rounded-b-sm flex items-center justify-between gap-2">
        {hadExisting ? (
          <button onClick={removeTransition} className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-warn px-1">
            Remove transition
          </button>
        ) : (
          <span />
        )}
        <div className="flex items-center gap-2">
          <button onClick={onClose} className="text-xs font-bold uppercase text-gig-muted hover:text-gig-text px-3 py-1.5">
            Close
          </button>
          <button
            onClick={save}
            disabled={saving}
            className="bg-gig-accent2 text-black rounded-sm px-4 py-1.5 text-xs font-bold uppercase tracking-wide disabled:opacity-60"
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CuePointSelect({
  label,
  emptyLabel,
  cues,
  value,
  onChange,
}: {
  label: string;
  emptyLabel: string;
  cues: HotCue[];
  value: number | null;
  onChange: (v: number | null) => void;
}) {
  return (
    <div className="flex-1 min-w-0">
      <label className="block text-[10px] text-gig-dim uppercase tracking-wide mb-0.5 truncate" title={label}>
        {label}
      </label>
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value === "" ? null : parseFloat(e.target.value))}
        className="w-full bg-black border border-gig-border rounded-sm px-2 py-1 text-xs"
      >
        <option value="">{emptyLabel}</option>
        {cues.map((c) => (
          <option key={c.id} value={c.position_secs}>
            {padLetter(c.pad)} · {c.label} · {formatClock(c.position_secs)}
          </option>
        ))}
      </select>
      {cues.length === 0 && (
        <p className="text-[10px] text-gig-dim mt-0.5">No hot cues — analyze this track first</p>
      )}
    </div>
  );
}
