const fs = require('fs');
const { createCanvas } = require('canvas');

// Create a 256x256 canvas
const canvas = createCanvas(256, 256);
const ctx = canvas.getContext('2d');

// Background - accent red/pink
ctx.fillStyle = '#e94560';
ctx.fillRect(0, 0, 256, 256);

// Rounded rectangle for camera body
ctx.fillStyle = '#ffffff';
roundRect(ctx, 40, 70, 176, 130, 20);
ctx.fill();

// Camera lens (outer circle)
ctx.beginPath();
ctx.arc(128, 135, 50, 0, Math.PI * 2);
ctx.fillStyle = '#e94560';
ctx.fill();

// Camera lens (inner circle)
ctx.beginPath();
ctx.arc(128, 135, 35, 0, Math.PI * 2);
ctx.fillStyle = '#ffffff';
ctx.fill();

// Camera lens (center dot)
ctx.beginPath();
ctx.arc(128, 135, 15, 0, Math.PI * 2);
ctx.fillStyle = '#e94560';
ctx.fill();

// Flash
ctx.fillStyle = '#ffffff';
roundRect(ctx, 170, 50, 30, 25, 5);
ctx.fill();

// Save as PNG
const buffer = canvas.toBuffer('image/png');
fs.writeFileSync(__dirname + '/icon.png', buffer);
console.log('Icon created!');

// Also create 32x32 version
const canvas32 = createCanvas(32, 32);
const ctx32 = canvas32.getContext('2d');
ctx32.drawImage(canvas, 0, 0, 32, 32);
fs.writeFileSync(__dirname + '/32x32.png', canvas32.toBuffer('image/png'));

// 128x128
const canvas128 = createCanvas(128, 128);
const ctx128 = canvas128.getContext('2d');
ctx128.drawImage(canvas, 0, 0, 128, 128);
fs.writeFileSync(__dirname + '/128x128.png', canvas128.toBuffer('image/png'));
fs.writeFileSync(__dirname + '/128x128@2x.png', buffer);

console.log('All icons created!');

function roundRect(ctx, x, y, width, height, radius) {
  ctx.beginPath();
  ctx.moveTo(x + radius, y);
  ctx.lineTo(x + width - radius, y);
  ctx.quadraticCurveTo(x + width, y, x + width, y + radius);
  ctx.lineTo(x + width, y + height - radius);
  ctx.quadraticCurveTo(x + width, y + height, x + width - radius, y + height);
  ctx.lineTo(x + radius, y + height);
  ctx.quadraticCurveTo(x, y + height, x, y + height - radius);
  ctx.lineTo(x, y + radius);
  ctx.quadraticCurveTo(x, y, x + radius, y);
  ctx.closePath();
}
