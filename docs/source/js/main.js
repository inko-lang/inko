(function() {
  "use strict";

  document.addEventListener('DOMContentLoaded', () => {
    document.querySelectorAll('.expand-menus a').forEach((button) => {
      button.addEventListener('click', (event) => {
        event.preventDefault();

        let query = button.dataset.toggle;
        let new_text = button.dataset.toggleText;
        let old_text = button.innerText;

        button.dataset.toggleText = old_text;
        button.innerText = new_text;
        document.querySelector(query).classList.toggle('visible');
      });
    });

    document.querySelectorAll('.highlight .copy').forEach((el) => {
      let timer = null;

      el.addEventListener('click', (e) => {
        e.preventDefault();

        if (timer) {
          clearTimeout(timer);
          timer = null;
        }

        el.classList.add('copied');
        navigator.clipboard.writeText(el.previousSibling.textContent);
        timer = setTimeout(function() { el.classList.remove('copied'); }, 2000);
      });
    });
  });
})();
