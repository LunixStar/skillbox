/** @type {import('tailwindcss').Config} */
const sb = {
  primary: "var(--sb-primary)",
  hover: "var(--sb-primary-hover)",
  active: "var(--sb-primary-active)",
  soft: "var(--sb-primary-soft)",
};

module.exports = {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // §11.1 PlainCraftLauncher2 式简洁白蓝设计令牌（正本在 index.css :root）
        primary: sb,
        highlight: "var(--sb-highlight)",
        page: "var(--sb-page)",
        surface: "var(--sb-surface)",
        line: "var(--sb-line)",
        ink: {
          DEFAULT: "var(--sb-ink)",
          secondary: "var(--sb-ink-secondary)",
          faint: "var(--sb-ink-faint)",
        },
        danger: "var(--sb-danger)",
        success: "var(--sb-success)",
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "Segoe UI",
          "Microsoft YaHei",
          "PingFang SC",
          "sans-serif",
        ],
      },
      borderRadius: {
        card: "var(--sb-radius-card)",
        btn: "var(--sb-radius-btn)",
        tag: "var(--sb-radius-tag)",
      },
      boxShadow: {
        card: "var(--sb-card-shadow)",
      },
      transitionDuration: {
        120: "120ms",
        200: "200ms",
      },
      transitionTimingFunction: {
        skillbox: "cubic-bezier(.2,.8,.2,1)",
      },
    },
  },
  plugins: [],
};
