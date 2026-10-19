/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // §11.1 PlainCraftLauncher2 式简洁白蓝设计令牌
        primary: {
          DEFAULT: "#3B8EF0",
          hover: "#2F7FD8",
          active: "#256FC4",
          soft: "#E8F2FF",
        },
        surface: "#F6F8FB",
        line: "#E3E9F0",
        ink: {
          DEFAULT: "#1B2430",
          secondary: "#5A6675",
          faint: "#8A94A3",
        },
        danger: "#E5544B",
        success: "#2FA46A",
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
    },
  },
  plugins: [],
};
