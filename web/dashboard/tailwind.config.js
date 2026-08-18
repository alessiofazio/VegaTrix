/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./app/**/*.{ts,tsx}", "./components/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#15232B",
        rail: "#1F7A6C",
        ticket: "#F6F1E8",
        signal: "#C4493B",
        ledger: "#5C6B73",
        paper: "#E7EEE9",
      },
      fontFamily: {
        sans: ["Instrument Sans", "Segoe UI", "sans-serif"],
        mono: ["IBM Plex Mono", "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
