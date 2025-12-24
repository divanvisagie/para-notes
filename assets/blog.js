// Add copy buttons to code blocks
(function () {
    document.querySelectorAll('pre').forEach(function (pre) {
        var button = document.createElement('button');
        button.className = 'copy-button';
        button.textContent = 'Copy';
        button.addEventListener('click', function () {
            var code = pre.querySelector('code');
            var text = code ? code.textContent : pre.textContent;
            navigator.clipboard.writeText(text).then(function () {
                button.textContent = 'Copied!';
                setTimeout(function () {
                    button.textContent = 'Copy';
                }, 2000);
            });
        });
        pre.style.position = 'relative';
        pre.appendChild(button);
    });
}());
