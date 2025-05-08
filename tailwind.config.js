
/** @type {import('tailwindcss').Config} */
export default {
    // The 'content' array tells Tailwind which files to scan
    // for classes. This is the most important part.
    content: [
      "./index.html", // Your main HTML file
      "./src/**/*.{vue,js,ts,jsx,tsx}" // All relevant files within your src folder
    ],
  
    // The 'theme' object is where you would customize
    // Tailwind's default design system (colors, fonts, spacing, etc.)
    // The 'extend' key allows you to add customizations without
    // overwriting the defaults entirely.
    theme: {
      extend: {
        // Example: add a custom color
        // colors: {
        //   'custom-blue': '#243c5a',
        // }
      },
    },
  
    // The 'plugins' array is where you add official or
    // third-party Tailwind plugins (e.g., @tailwindcss/forms)
    plugins: [
      // Example: require('@tailwindcss/forms'),
    ],
  }