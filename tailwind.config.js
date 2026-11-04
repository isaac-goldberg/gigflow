/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Gigflow brand, lifted from the pitch deck: black stage background,
        // neon lime as the signature accent, magenta/blue as secondary signal
        // colors (hot cues, warnings, link states) — not a generic SaaS palette.
        gig: {
          bg: "#060607",
          panel: "#131315",
          panel2: "#1b1b1e",
          panel3: "#242427",
          border: "#303033",
          borderLight: "#3d3d41",
          accent: "#9bff1f",
          accent2: "#ff2ea6",
          accent3: "#2e7bff",
          warn: "#ff3b3b",
          text: "#eceef0",
          muted: "#8c8e93",
          dim: "#58595d",
        },
      },
      fontFamily: {
        // IBM Plex Sans, self-hosted via @fontsource (see index.css) —
        // neutral, engineered, pairs with the Plex Mono below. Works offline.
        sans: ["IBM Plex Sans", "Helvetica Neue", "Arial", "sans-serif"],
        mono: ["Menlo", "Consolas", "ui-monospace", "SFMono-Regular", "monospace"],
      },
      borderRadius: {
        none: "0px",
        DEFAULT: "2px",
        sm: "2px",
        md: "3px",
        lg: "4px",
        xl: "4px",
        "2xl": "4px",
        full: "9999px",
      },
    },
  },
  plugins: [],
}
