module.exports = {
  content: [
    "./pages/**/*.{js,ts,jsx,tsx}",
    "./components/**/*.{js,ts,jsx,tsx}",
    "./app/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        sketch: {
          black: '#2c2c2c',
          gray: '#666666',
          light: '#e8e6e1',
          accent: '#4a7c59',
          'accent-light': '#7ba378',
          background: '#faf8f5',
          card: '#ffffff',
        },
      },
      fontFamily: {
        handwriting: ['"Caveat"', 'cursive'],
        cave: ['"Caveat"', 'cursive'],
        marker: ['"Permanent Marker"', 'cursive'],
      },
      backgroundImage: {
        'sketch-gradient': 'linear-gradient(135deg, #2c2c2c, #666666)',
      },
      boxShadow: {
        'sketch': '3px 3px 0 rgba(44, 44, 44, 0.15)',
        'sketch-lg': '4px 4px 0 rgba(44, 44, 44, 0.2)',
        'sketch-sm': '2px 2px 0 rgba(44, 44, 44, 0.1)',
      },
      borderRadius: {
        'sketch': '4px',
      },
      borderWidth: {
        'sketch': '2.5px',
      },
    },
  },
  plugins: [],
}
