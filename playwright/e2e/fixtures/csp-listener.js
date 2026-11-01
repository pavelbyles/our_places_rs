// Loaded with page.addInitScript({ path }) on the redesigned-portal projects.
// Reports every Content-Security-Policy violation to the test runner.
document.addEventListener('securitypolicyviolation', (event) => {
  const report = window.__reportCspViolation;
  if (typeof report === 'function') {
    report(`${event.violatedDirective} blocked=${event.blockedURI || 'inline'} at ${event.sourceFile || document.location.href}:${event.lineNumber}`);
  }
});
