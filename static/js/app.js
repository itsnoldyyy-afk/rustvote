document.querySelectorAll("form[data-loading]").forEach((form) => {
    form.addEventListener("submit", () => {
        const button = form.querySelector("button[type='submit']");
        if (button) {
            button.disabled = true;
            button.textContent = "Working...";
        }
    });
});
