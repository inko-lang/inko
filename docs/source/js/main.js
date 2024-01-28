(function() {
  "use strict";

  document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.expand-menus a').forEach(function(button) {
      button.addEventListener('click', function(event) {
        event.preventDefault();

        let query = button.dataset.toggle;
        let new_text = button.dataset.toggleText;
        let old_text = button.innerText;

        button.dataset.toggleText = old_text;
        button.innerText = new_text;
        document.querySelector(query).classList.toggle('visible');
      });
    });
  });
})();
