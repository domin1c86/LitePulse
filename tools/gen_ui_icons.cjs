// Run with Node.js and sharp available through NODE_PATH.
// The application embeds these 64x64 alpha masks; no SVG runtime is needed.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const root = path.join(__dirname, '..');
const icons = { cpu: 'CPU.svg', memory: 'memory.svg', gpu: 'graphic.svg', disk: 'SSD.svg', up: 'up.svg', down: 'down.svg', pin: '.svg' };
(async () => {
  fs.mkdirSync(path.join(root, 'assets', 'ui'), { recursive: true });
  for (const [name, source] of Object.entries(icons)) {
    // Some supplied arrows extend beyond their viewBox. Normalize their bounds
    // so that the full arrow remains visible, including its arrowhead.
    let svg = fs.readFileSync(path.join(root, 'images', source), 'utf8');
    if (name === 'up') svg = svg.replace('0 0 1024 1024', '0 0 1024 1120');
    const alpha = await sharp(Buffer.from(svg), { density: 384 }).resize(64, 64).ensureAlpha().extractChannel(3).raw().toBuffer();
    fs.writeFileSync(path.join(root, 'assets', 'ui', name + '.alpha'), alpha);
  }
})();
