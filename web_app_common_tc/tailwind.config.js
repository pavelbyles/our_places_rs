/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: 'class',
  content: [
    "./src/**/*.{rs,html}",
    "../web_app_tc/src/**/*.{rs,html}",
    "../web_app_admin_tc/src/**/*.{rs,html}",
  ],
  daisyui: {
    themes: ["emerald", "sunset"],
    darkTheme: "sunset",
  },
  plugins: [],
}
