/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        bg: {
          primary: '#0d1117',
          secondary: '#161b22',
          tertiary: '#21262d',
        },
        border: {
          DEFAULT: '#30363d',
          hover: '#484f58',
        },
        text: {
          primary: '#e6edf3',
          secondary: '#8b949e',
        },
        accent: '#58a6ff',
      },
    },
  },
  plugins: [],
}
