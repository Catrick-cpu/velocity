(() => {
  const canvas = document.getElementById('world');
  const ctx = canvas.getContext('2d');
  const ui = {
    startBtn: document.getElementById('startBtn'),
    pauseBtn: document.getElementById('pauseBtn'),
    resetBtn: document.getElementById('resetBtn'),
    routeSelect: document.getElementById('routeSelect'),
    quality: document.getElementById('quality'),
    scale: document.getElementById('scale'),
    viewDist: document.getElementById('viewDist'),
    veg: document.getElementById('veg'),
    timeOfDay: document.getElementById('timeOfDay'),
    cloud: document.getElementById('cloud'),
    rain: document.getElementById('rain'),
    fog: document.getElementById('fog'),
    ultraWarn: document.getElementById('ultraWarn'),
    fps: document.getElementById('fps'),
    frameMs: document.getElementById('frameMs'),
    presetStat: document.getElementById('presetStat'),
    qualityLabel: document.getElementById('qualityLabel'),
    weatherLabel: document.getElementById('weatherLabel'),
    routeName: document.getElementById('routeName'),
    speed: document.getElementById('speed'),
    power: document.getElementById('power'),
    cadence: document.getElementById('cadence'),
    hr: document.getElementById('hr'),
    distance: document.getElementById('distance'),
    time: document.getElementById('time'),
    grade: document.getElementById('grade'),
  };

  const state = {
    running: false,
    t: 0,
    last: performance.now(),
    fpsAccum: 0,
    fpsFrames: 0,
    fpsLast: performance.now(),
    distance: 0, // meters
    speed: 0, // m/s
    power: 180,
    effort: 1,
    camera: 'chase',
    routeKey: 'alpine',
    seed: 1337,
  };

  const routes = {
    alpine: { name: 'ALPINE PASS (DEMO)', length: 18200, climb: 620, base: 720, hue: 112 },
    lakeside: { name: 'LAKESIDE LOOP (DEMO)', length: 14500, climb: 240, base: 430, hue: 190 },
    forest: { name: 'FOREST VALLEY (DEMO)', length: 12100, climb: 310, base: 520, hue: 128 },
  };

  function clamp(v, a, b) { return Math.max(a, Math.min(b, v)); }
  function lerp(a, b, t) { return a + (b - a) * t; }
  function smoothstep(e0, e1, x) { const t = clamp((x - e0) / (e1 - e0 || 1), 0, 1); return t * t * (3 - 2 * t); }
  function fract(x) { return x - Math.floor(x); }
  function hash(n) { return fract(Math.sin(n) * 43758.5453123); }
  function noise1(x) {
    const i = Math.floor(x), f = fract(x), u = f * f * (3 - 2 * f);
    return lerp(hash(i), hash(i + 1), u);
  }
  function fbm(x) {
    let v = 0, a = 0.5, f = 1;
    for (let i = 0; i < 4; i++) { v += a * (noise1(x * f) * 2 - 1); f *= 2.02; a *= 0.5; }
    return v;
  }

  function routeProfile(s, route) {
    const d = (s / route.length) % 1;
    const x = d * Math.PI * 2;
    const hills = Math.sin(x * 1.0) * 0.55 + Math.sin(x * 2.7 + 1.2) * 0.28 + Math.sin(x * 5.4 + 0.4) * 0.12;
    const elev = route.base + route.climb * (0.5 + 0.5 * hills);
    const grade = (Math.cos(x) * 0.55 * 1.0 + Math.cos(x * 2.7 + 1.2) * 0.28 * 2.7 + Math.cos(x * 5.4 + 0.4) * 0.12 * 5.4) * 8.5;
    const bend = Math.sin(x * 1.8) * 0.8 + Math.sin(x * 4.1 + 0.7) * 0.25;
    return { elev, grade, bend, d };
  }

  function resize() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const scale = (+ui.scale.value / 100) * (ui.quality.value === 'Ultra' ? 1 : ui.quality.value === 'High' ? 0.95 : 1);
    const rect = canvas.getBoundingClientRect();
    canvas.width = Math.max(320, Math.floor(rect.width * dpr * scale));
    canvas.height = Math.max(180, Math.floor(rect.height * dpr * scale));
  }

  function skyColors(hour, cloud, rain) {
    const day = clamp(Math.sin(((hour - 6) / 12) * Math.PI), 0, 1);
    const dusk = clamp(1 - Math.abs(hour - 18) / 3, 0, 1) * 0.6;
    const storm = rain / 100;
    const c = cloud / 100;
    const top = [lerp(8, 70, day) - c * 25 - storm * 20, lerp(16, 130, day) - c * 30 - storm * 25, lerp(30, 210, day) - c * 35 - storm * 30];
    const hor = [lerp(20, 140, day) + dusk * 90 - c * 20, lerp(24, 170, day) + dusk * 40 - c * 25, lerp(40, 220, day) - dusk * 20 - c * 30];
    return { day, top: `rgb(${top|0},${top[1]|0},${top[2]|0})`, hor: `rgb(${hor|0},${hor[1]|0},${hor[2]|0})` };
  }

  function draw() {
    const now = performance.now();
    const dt = Math.min(0.05, (now - state.last) / 1000);
    state.last = now;
    if (state.running) {
      state.t += dt;
      const route = routes[state.routeKey];
      const p = routeProfile(state.distance, route);
      const targetPower = 140 + 180 * state.effort;
      const gradeFactor = 1 + p.grade * 0.03;
      const drag = 0.22 + (ui.rain.value / 100) * 0.05;
      const accel = (targetPower / 75) / Math.max(0.6, gradeFactor) - drag * state.speed - 0.15 * Math.max(0, p.grade);
      state.speed = clamp(state.speed + accel * dt * 0.6, 0, 22);
      state.distance = (state.distance + state.speed * dt) % route.length;
      state.power = Math.round(lerp(state.power, targetPower + p.grade * 6, 0.08));
    } else {
      state.speed = Math.max(0, state.speed - dt * 1.5);
    }

    const route = routes[state.routeKey];
    const prof = routeProfile(state.distance, route);
    const hour = +ui.timeOfDay.value;
    const cloud = +ui.cloud.value;
    const rain = +ui.rain.value;
    const fogAmt = +ui.fog.value;
    const quality = ui.quality.value;
    const preset = quality === 'Auto' ? 'Auto' : quality;
    const view = (+ui.viewDist.value / 100) * (quality === 'Ultra' ? 1.25 : quality === 'High' ? 1.1 : quality === 'Low' || quality === 'Ultra Low' ? 0.75 : 1.0);
    const vegDensity = (+ui.veg.value / 100) * (quality === 'Ultra' ? 1.6 : quality === 'High' ? 1.2 : 1.0);
    ui.ultraWarn.classList.toggle('hidden', quality !== 'Ultra');
    ui.presetStat.textContent = preset;
    ui.qualityLabel.textContent = preset.toUpperCase();
    ui.weatherLabel.textContent = rain > 55 ? 'RAIN' : cloud > 60 ? 'OVERCAST' : fogAmt > 45 ? 'FOG' : 'CLEAR';
    ui.routeName.textContent = route.name;

    const { day, top, hor } = skyColors(hour, cloud, rain);
    const w = canvas.width, h = canvas.height;
    const g = ctx.createLinearGradient(0, 0, 0, h);
    g.addColorStop(0, top); g.addColorStop(0.55, hor); g.addColorStop(1, `rgb(${30 + day*20},${40 + day*25},${35 + day*15})`);
    ctx.fillStyle = g; ctx.fillRect(0, 0, w, h);

    // sun/moon
    const sunT = (hour - 6) / 12;
    const sunX = w * (0.1 + 0.8 * clamp(sunT, 0, 1));
    const sunY = h * (0.55 - 0.38 * Math.sin(clamp(sunT, 0, 1) * Math.PI));
    ctx.globalAlpha = day > 0.02 ? 0.95 : 0.7;
    ctx.beginPath(); ctx.arc(sunX, sunY, Math.max(10, w*0.018), 0, Math.PI*2);
    ctx.fillStyle = day > 0.05 ? `rgba(255,${220 - cloud},${170 - cloud*0.6},0.95)` : 'rgba(220,230,255,0.8)';
    ctx.fill(); ctx.globalAlpha = 1;

    // distant mountains
    const horizonY = h * 0.48;
    for (let layer = 0; layer < 3; layer++) {
      const depth = 1 + layer * 0.8;
      const baseY = horizonY + layer * h * 0.06;
      const amp = h * (0.16 - layer * 0.03) * (0.7 + route.climb / 1200);
      ctx.beginPath();
      ctx.moveTo(0, h);
      for (let x = 0; x <= w; x += 12) {
        const wx = (x / w) * 6 + state.distance * 0.00002 / depth + layer * 10;
        const n = fbm(wx * 0.8) * 0.6 + fbm(wx * 1.7 + 5) * 0.4;
        const y = baseY - amp * (0.45 + 0.55 * (n * 0.5 + 0.5)) - prof.elev * 0.01 / depth;
        if (x === 0) ctx.lineTo(0, y); else ctx.lineTo(x, y);
      }
      ctx.lineTo(w, h); ctx.closePath();
      const shade = lerp(40, 90, day) - layer * 18 - cloud * 0.15;
      ctx.fillStyle = `rgb(${shade|0},${(shade+18)|0},${(shade+8)|0})`;
      ctx.fill();
    }

    // ground / road perspective
    const camModes = {
      chase: { height: 1.0, look: 0.22, fov: 1.0 },
      close: { height: 0.8, look: 0.18, fov: 1.12 },
      hood: { height: 0.55, look: 0.12, fov: 1.18 },
      drone: { height: 2.2, look: 0.34, fov: 0.85 },
    };
    const cam = camModes[state.camera] || camModes.chase;
    const roadWNear = w * (state.camera === 'hood' ? 0.62 : 0.42) * cam.fov;
    const stripes = Math.floor(24 * view);
    const wet = rain / 100;
    for (let i = stripes; i >= 1; i--) {
      const z = i / stripes;
      const pz = Math.pow(z, 1.7);
      const y = horizonY + (h - horizonY) * pz;
      const nextZ = (i - 1) / stripes;
      const nextPz = Math.pow(nextZ, 1.7);
      const y2 = horizonY + (h - horizonY) * nextPz;
      const segS = state.distance + z * 420 * view;
      const seg = routeProfile(segS, route);
      const curve = (seg.bend - prof.bend) * w * 0.18 * (1 - pz);
      const cx = w * 0.5 + curve;
      const halfW = roadWNear * (1 - pz) * 0.5;
      const grassShade = (90 + day * 60 - cloud * 0.2) * (0.7 + 0.3 * ((i % 2)));
      const rcol = 55 + day * 20 + wet * 18;
      // shoulders/grass strips
      ctx.fillStyle = `rgb(${(grassShade*0.55)|0},${(grassShade*0.85)|0},${(grassShade*0.45)|0})`;
      ctx.fillRect(0, y2, w, Math.max(1, y - y2 + 1));
      // road
      ctx.beginPath();
      ctx.moveTo(cx - halfW, y); ctx.lineTo(cx + halfW, y);
      ctx.lineTo(cx + halfW * 0.82, y2); ctx.lineTo(cx - halfW * 0.82, y2);
      ctx.closePath();
      ctx.fillStyle = `rgb(${rcol|0},${(rcol+2)|0},${(rcol+6)|0})`;
      ctx.fill();
      // center line
      if (i % 3 === 0) {
        ctx.strokeStyle = `rgba(240,235,210,${0.55 * (1-pz)})`;
        ctx.lineWidth = Math.max(1, w * 0.003 * (1-pz));
        ctx.beginPath(); ctx.moveTo(cx, y); ctx.lineTo(cx + (curve * -0.05), y2); ctx.stroke();
      }
      // edge lines
      ctx.strokeStyle = `rgba(230,230,230,${0.35 * (1-pz)})`;
      ctx.lineWidth = Math.max(1, w * 0.002 * (1-pz));
      ctx.beginPath(); ctx.moveTo(cx - halfW*0.96, y); ctx.lineTo(cx - halfW*0.79, y2); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(cx + halfW*0.96, y); ctx.lineTo(cx + halfW*0.79, y2); ctx.stroke();
      // vegetation/buildings instances by distance
      if (vegDensity > 0.05 && i % 2 === 0) {
        const sideCount = Math.floor((quality === 'Ultra' ? 3 : quality === 'High' ? 2 : 1) * (0.4 + vegDensity));
        for (let s = 0; s < sideCount; s++) {
          const rnd = hash(Math.floor(segS*0.02) * 31 + s * 7 + i * 13);
          if (rnd < 0.55) {
            const side = rnd > 0.275 ? 1 : -1;
            const ox = halfW * (1.25 + rnd * 1.8) * side;
            const tx = cx + ox;
            const size = (1 - pz) * h * (0.03 + rnd * 0.05) * (quality === 'Ultra' ? 1.2 : 1);
            if (tx > -50 && tx < w + 50 && y > horizonY) {
              // trees or houses occasionally
              if (rnd > 0.48 && pz < 0.55 && quality !== 'Ultra Low') {
                // simple house
                const bw = size * 1.4, bh = size * 1.1;
                ctx.fillStyle = `rgba(${210-rnd*30},${205-rnd*25},${190-rnd*20},${0.85 - pz*0.5})`;
                ctx.fillRect(tx - bw/2, y - bh, bw, bh);
                ctx.fillStyle = `rgba(120,70,60,${0.85 - pz*0.5})`;
                ctx.beginPath(); ctx.moveTo(tx-bw*0.55, y-bh); ctx.lineTo(tx, y-bh-size*0.7); ctx.lineTo(tx+bw*0.55, y-bh); ctx.closePath(); ctx.fill();
              } else {
                const trunkH = size * 0.5;
                ctx.fillStyle = `rgba(90,60,40,${0.8 - pz*0.4})`;
                ctx.fillRect(tx - size*0.06, y - trunkH, size*0.12, trunkH);
                ctx.fillStyle = `rgba(${30+route.hue*0.1},${70 + day*30 - pz*30},${35},${0.8 - pz*0.35})`;
                ctx.beginPath(); ctx.arc(tx, y - trunkH - size*0.25, size*0.38, 0, Math.PI*2); ctx.fill();
              }
            }
          }
        }
      }
    }

    // cyclist
    const riderY = h * (state.camera === 'drone' ? 0.72 : 0.78);
    const riderX = w * 0.5 + (prof.bend - prof.bend) * 0;
    const bob = Math.sin(state.t * (4 + state.speed)) * 2;
    ctx.save();
    ctx.translate(riderX, riderY + bob);
    const s = w * 0.0012 * (state.camera === 'close' ? 1.2 : state.camera === 'hood' ? 0.0 : 1.0);
    if (state.camera !== 'hood') {
      ctx.strokeStyle = 'rgba(20,25,30,0.95)'; ctx.lineWidth = 4*s; ctx.lineCap='round';
      // wheels
      ctx.beginPath(); ctx.arc(-18*s, 0, 16*s, 0, Math.PI*2); ctx.stroke();
      ctx.beginPath(); ctx.arc(18*s, 0, 16*s, 0, Math.PI*2); ctx.stroke();
      // frame
      ctx.beginPath(); ctx.moveTo(-18*s,0); ctx.lineTo(0,-18*s); ctx.lineTo(18*s,0); ctx.lineTo(2*s,-26*s); ctx.lineTo(-8*s,-10*s); ctx.stroke();
      // rider
      ctx.strokeStyle = 'rgba(230,240,245,0.95)';
      ctx.beginPath(); ctx.moveTo(0,-18*s); ctx.lineTo(-4*s,-36*s); ctx.lineTo(10*s,-40*s); ctx.stroke();
      ctx.beginPath(); ctx.arc(12*s,-46*s, 5*s, 0, Math.PI*2); ctx.stroke();
    }
    ctx.restore();

    // rain / fog overlays
    if (rain > 2) {
      ctx.strokeStyle = `rgba(180,200,220,${0.08 + rain*0.002})`;
      ctx.lineWidth = 1;
      const drops = Math.floor(80 + rain * 2);
      for (let i=0;i<drops;i++) {
        const x = (hash(i*17 + Math.floor(state.t*20)) * w + state.t*120) % w;
        const y = (hash(i*29 + 3) * h + state.t*900) % h;
        ctx.beginPath(); ctx.moveTo(x,y); ctx.lineTo(x+2, y+10); ctx.stroke();
      }
      ctx.fillStyle = `rgba(40,55,70,${rain*0.0012})`; ctx.fillRect(0,h*0.55,w,h*0.45);
    }
    if (fogAmt > 0) {
      ctx.fillStyle = `rgba(210,220,230,${fogAmt*0.0022 * (1-day*0.4)})`;
      ctx.fillRect(0, horizonY - h*0.05, w, h*0.6);
    }

    // HUD values
    ui.speed.textContent = Math.round(state.speed * 3.6).toString();
    ui.power.textContent = `${state.running ? state.power : 0} W`;
    ui.cadence.textContent = `${state.running ? Math.round(70 + state.speed*2 + state.effort*10) : 0} rpm`;
    ui.hr.textContent = `${state.running ? Math.round(110 + state.effort*45 + state.speed*1.2) : '--'} bpm`;
    ui.distance.textContent = (state.distance / 1000).toFixed(2);
    const secs = Math.floor(state.t);
    ui.time.textContent = `${String(Math.floor(secs/60)).padStart(2,'0')}:${String(secs%60).padStart(2,'0')}`;
    ui.grade.textContent = `${prof.grade.toFixed(1)}%`;

    // perf
    state.fpsAccum += dt; state.fpsFrames++;
    if (now - state.fpsLast > 500) {
      const fps = state.fpsFrames / state.fpsAccum;
      ui.fps.textContent = fps.toFixed(0);
      ui.frameMs.textContent = `${(1000 / Math.max(1,fps)).toFixed(1)} ms`;
      state.fpsAccum = 0; state.fpsFrames = 0; state.fpsLast = now;
    }

    requestAnimationFrame(draw);
  }

  function setCamera(cam) {
    state.camera = cam;
    document.querySelectorAll('.cam').forEach(b => b.classList.toggle('active', b.dataset.cam === cam));
  }

  ui.startBtn.addEventListener('click', () => { state.running = true; });
  ui.pauseBtn.addEventListener('click', () => { state.running = !state.running; ui.pauseBtn.textContent = state.running ? 'Pause' : 'Resume'; });
  ui.resetBtn.addEventListener('click', () => { state.running = false; state.t = 0; state.distance = 0; state.speed = 0; ui.pauseBtn.textContent = 'Pause'; });
  ui.routeSelect.addEventListener('change', () => { state.routeKey = ui.routeSelect.value; state.distance = 0; });
  document.querySelectorAll('.cam').forEach(b => b.addEventListener('click', () => setCamera(b.dataset.cam)));
  [ui.quality, ui.scale, ui.viewDist, ui.veg, ui.timeOfDay, ui.cloud, ui.rain, ui.fog].forEach(el => el.addEventListener('input', resize));
  window.addEventListener('resize', resize);
  window.addEventListener('keydown', (e) => {
    if (e.code === 'Space') { e.preventDefault(); state.running = !state.running; ui.pauseBtn.textContent = state.running ? 'Pause' : 'Resume'; }
    if (e.key === 'r' || e.key === 'R') ui.resetBtn.click();
    if (e.key === '1') setCamera('chase');
    if (e.key === '2') setCamera('close');
    if (e.key === '3') setCamera('hood');
    if (e.key === '4') setCamera('drone');
    if (e.key === '+' || e.key === '=') state.effort = clamp(state.effort + 0.1, 0.3, 1.6);
    if (e.key === '-' || e.key === '_') state.effort = clamp(state.effort - 0.1, 0.3, 1.6);
  });

  resize();
  requestAnimationFrame(draw);
})();
