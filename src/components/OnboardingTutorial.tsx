import { useEffect, useState } from "react";
import type { View } from "./Sidebar";
import { useEscapeKey } from "../lib/useEscapeKey";

interface Step {
  title: string;
  body: string;
  /** Switch to this app view when the step starts, so the target element is on screen. */
  view?: View;
  /** data-tour selector of the real button this step points at, if any. */
  selector?: string;
}

const STEPS: Step[] = [
  {
    title: "Welcome to Gigflow",
    body: "Gigflow helps you prep a DJ set: import tracks, auto-detect BPM/key/energy, build a setlist with harmonic-mix suggestions, and export it. We'll point out exactly what to click as we go — let's build your first set with just two songs.",
  },
  {
    title: "1. Import two songs",
    body: 'Click the highlighted "+ Import sound files" button and pick any two tracks — MP3, WAV, FLAC, AAC/M4A, OGG or AIFF all work. They\'ll show up in your library right away.',
    view: "library",
    selector: '[data-tour="import-button"]',
  },
  {
    title: "2. Analyze them",
    body: 'Each new track needs analysis before Gigflow knows its BPM, key and energy. Click "Analyze all" (shown here once you have unanalyzed tracks) to do every unanalyzed track at once — this also detects hot cues automatically.',
    view: "library",
    selector: '[data-tour="analyze-all-button"]',
  },
  {
    title: "3. Create a setlist",
    body: 'Click "+ New setlist", give it a name, and hit Create — this is where you\'ll arrange your two songs in play order.',
    view: "setlists",
    selector: '[data-tour="new-setlist-button"]',
  },
  {
    title: "4. Add your two tracks",
    body: 'Inside your new setlist, click "+ Add track" and pick your first song, then do it again for your second. Drag the ⠿ handle to reorder if you want to swap which plays first.',
    selector: '[data-tour="add-track-button"]',
  },
  {
    title: "5. Check the mix compatibility",
    body: 'Between your two tracks you\'ll see their BPM difference and Camelot key relationship — a quick read on how well they\'ll blend. Click "Suggest next" on a track to see other library tracks ranked by how well they\'d follow it.',
    selector: '[data-tour="suggest-next-button"]',
  },
  {
    title: "6. Assign and hear a transition",
    body: 'Click "+ assign transition" between the two tracks to save a blend (type, length in bars, cue points). The pink boxes on each track show where the mix happens. Press ▶ on the first track and the next one is beatmatched and mixed in live when it reaches the zone. Untick "Enabled" to hear the first track on its own.',
    selector: '[data-tour="assign-transition-button"]',
  },
  {
    title: "7. Export",
    body: 'When you\'re happy with it, click "Export…" to copy the audio files plus a Rekordbox XML (with your hot cues and transition markers) to a USB drive or folder. You\'re ready to play.',
    selector: '[data-tour="export-button"]',
  },
];

interface SpotRect {
  top: number;
  left: number;
  width: number;
  height: number;
}

const SPOT_PAD = 6;
const CARD_WIDTH = 320;
const CARD_GAP = 14;

export function OnboardingTutorial({
  onClose,
  onChangeView,
}: {
  onClose: () => void;
  onChangeView: (v: View) => void;
}) {
  const [step, setStep] = useState(0);
  const [spot, setSpot] = useState<SpotRect | null>(null);
  const last = step === STEPS.length - 1;
  const current = STEPS[step];
  useEscapeKey(onClose);

  useEffect(() => {
    if (current.view) onChangeView(current.view);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step]);

  // Re-measures every frame while a step targets a real element, so the
  // cutout tracks layout shifts, scrolling and the view switch above —
  // simpler and more robust than wiring up resize/scroll listeners.
  useEffect(() => {
    if (!current.selector) {
      setSpot(null);
      return;
    }
    let raf = 0;
    const selector = current.selector;
    function measure() {
      const el = document.querySelector(selector);
      if (el) {
        const r = el.getBoundingClientRect();
        setSpot({
          top: r.top - SPOT_PAD,
          left: r.left - SPOT_PAD,
          width: r.width + SPOT_PAD * 2,
          height: r.height + SPOT_PAD * 2,
        });
      } else {
        setSpot(null);
      }
      raf = requestAnimationFrame(measure);
    }
    measure();
    return () => cancelAnimationFrame(raf);
  }, [step, current.selector]);

  function next() {
    if (last) onClose();
    else setStep((s) => s + 1);
  }
  function back() {
    setStep((s) => Math.max(0, s - 1));
  }

  if (!spot) {
    // No real element to point at yet (or this step is purely informational)
    // — fall back to a centered card, same as the old static tutorial.
    return (
      <div className="fixed inset-0 bg-black/80 flex items-center justify-center z-50" onClick={onClose}>
        <div onClick={(e) => e.stopPropagation()}>
          <TourCard step={step} current={current} last={last} onBack={back} onNext={next} onSkip={onClose} />
        </div>
      </div>
    );
  }

  const cardBelow = spot.top < window.innerHeight * 0.6;
  const cardTop = cardBelow ? spot.top + spot.height + CARD_GAP : undefined;
  const cardBottom = cardBelow ? undefined : window.innerHeight - spot.top + CARD_GAP;
  const cardLeft = Math.min(Math.max(spot.left, 16), window.innerWidth - CARD_WIDTH - 16);

  return (
    <div className="fixed inset-0 z-50">
      {/* Four bands around the cutout: this is the whole trick — nothing is
          rendered over the spotlighted rect itself, so the real button stays
          genuinely clickable through the overlay. */}
      <div
        className="fixed bg-black/75"
        style={{ top: 0, left: 0, right: 0, height: Math.max(0, spot.top) }}
        onClick={onClose}
      />
      <div
        className="fixed bg-black/75"
        style={{ top: spot.top + spot.height, left: 0, right: 0, bottom: 0 }}
        onClick={onClose}
      />
      <div
        className="fixed bg-black/75"
        style={{ top: spot.top, height: spot.height, left: 0, width: Math.max(0, spot.left) }}
        onClick={onClose}
      />
      <div
        className="fixed bg-black/75"
        style={{ top: spot.top, height: spot.height, left: spot.left + spot.width, right: 0 }}
        onClick={onClose}
      />
      <div
        className="fixed border-2 border-gig-accent rounded-sm pointer-events-none"
        style={{
          top: spot.top,
          left: spot.left,
          width: spot.width,
          height: spot.height,
          boxShadow: "0 0 0 4px rgba(155,255,31,0.25)",
        }}
      />
      <div className="fixed" style={{ top: cardTop, bottom: cardBottom, left: cardLeft, width: CARD_WIDTH }}>
        <TourCard step={step} current={current} last={last} onBack={back} onNext={next} onSkip={onClose} />
      </div>
    </div>
  );
}

function TourCard({
  step,
  current,
  last,
  onBack,
  onNext,
  onSkip,
}: {
  step: number;
  current: Step;
  last: boolean;
  onBack: () => void;
  onNext: () => void;
  onSkip: () => void;
}) {
  return (
    <div className="bg-gig-panel border border-gig-borderLight rounded-sm w-full max-w-md flex flex-col shadow-lg">
      <div className="p-4 border-b border-gig-border bg-gig-panel2 flex items-center justify-between">
        <span className="text-xs font-bold uppercase tracking-wider text-gig-accent">Getting started</span>
        <button onClick={onSkip} className="text-[11px] font-bold uppercase text-gig-muted hover:text-gig-text">
          Skip
        </button>
      </div>

      <div className="p-5 min-h-[140px]">
        <h2 className="text-base font-bold mb-2">{current.title}</h2>
        <p className="text-sm text-gig-text/90 leading-relaxed">{current.body}</p>
      </div>

      <div className="flex items-center justify-center gap-1.5 pb-3">
        {STEPS.map((_, i) => (
          <span
            key={i}
            className="block w-1.5 h-1.5 rounded-full"
            style={{ backgroundColor: i === step ? "#9bff1f" : "#303033" }}
          />
        ))}
      </div>

      <div className="p-3 border-t border-gig-border flex justify-between bg-gig-panel2">
        <button
          disabled={step === 0}
          onClick={onBack}
          className="text-xs font-bold uppercase text-gig-muted hover:text-gig-text disabled:opacity-30 px-3 py-1.5"
        >
          Back
        </button>
        <button
          onClick={onNext}
          className="text-xs font-bold uppercase px-4 py-1.5 rounded-sm bg-gig-accent text-black hover:bg-gig-accent/80"
        >
          {last ? "Let's go" : "Next"}
        </button>
      </div>
    </div>
  );
}
