/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        // 与 app-icon.png 的主色 #28559C 对齐
        primary: {
          50: '#eff4fb',
          100: '#dce7f5',
          200: '#bccfe9',
          300: '#91afd8',
          400: '#5f88c4',
          500: '#3b6aaf',
          600: '#28559c',
          700: '#204680',
          800: '#1b3a69',
          900: '#172f55',
          950: '#0e1d35',
        },
      },
    },
  },
  plugins: [],
};
