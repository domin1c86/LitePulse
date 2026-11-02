// Run with Node.js and sharp available through NODE_PATH.
// The application embeds these 64x64 alpha masks; no SVG runtime is needed.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const root = path.join(__dirname, '..');
const icons = { cpu: 'CPU.svg', memory: 'memory.svg', gpu: 'graphic.svg', disk: 'SSD.svg', up: 'up.svg', down: 'down.svg', pin: '.svg', close: 'close.svg', settings: 'setting.svg' };
(async () => {
  fs.mkdirSync(path.join(root, 'assets', 'ui'), { recursive: true });
  for (const [name, source] of Object.entries(icons)) {
    const svg = fs.readFileSync(path.join(root, 'images', source), 'utf8');
    let image = sharp(Buffer.from(svg), { density: 384 });
    // Normalize visible bounds so header icons align and both network arrows
    // occupy matching boxes regardless of the source SVG's transparent margins.
    if (['pin', 'close', 'settings', 'up', 'down'].includes(name)) image = image.trim();
    const alpha = await image.resize(64, 64, { fit: 'contain', background: { r: 0, g: 0, b: 0, alpha: 0 } }).ensureAlpha().extractChannel(3).raw().toBuffer();
    fs.writeFileSync(path.join(root, 'assets', 'ui', name + '.alpha'), alpha);
  }
})();
