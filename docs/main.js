// détends — the landing page.
//
// Two small behaviours and nothing else. A page about a calm system should not
// ship a framework to fade some headings in.

(function () {
  "use strict";

  const sections = document.querySelectorAll(".reveal");

  // Anyone who has asked for less motion gets the content immediately, with no
  // transition to wait through.
  const stillness = window.matchMedia("(prefers-reduced-motion: reduce)");

  if (!("IntersectionObserver" in window) || stillness.matches) {
    sections.forEach((section) => section.classList.add("shown"));
    return;
  }

  const observer = new IntersectionObserver(
    (entries) => {
      entries.forEach((entry) => {
        if (!entry.isIntersecting) return;
        entry.target.classList.add("shown");
        // Reveal once. A section that fades out again as you scroll back is
        // the page demanding attention it has already had.
        observer.unobserve(entry.target);
      });
    },
    { rootMargin: "0px 0px -12% 0px", threshold: 0.08 }
  );

  sections.forEach((section) => observer.observe(section));

  // Anything already on screen at load should not wait for a scroll that may
  // never come.
  requestAnimationFrame(() => {
    sections.forEach((section) => {
      if (section.getBoundingClientRect().top < window.innerHeight) {
        section.classList.add("shown");
      }
    });
  });
})();
