"use strict";

/* UndiBukti demo page wiring. All rules live in sim.js (UndiBuktiSim). */

const Sim = window.UndiBuktiSim;
const DEMO_ENTRANTS = ["alice", "bob", "carol", "dave", "erin"];
const ORGANIZER = "organizer";

let draw = null; // current Sim state, or null before creation
let lastDrawResult = null; // { epochHash, seed, winnerIndex, winner }

const $ = (id) => document.getElementById(id);

function log(message, ok = true) {
  const li = document.createElement("li");
  li.className = ok ? "ok" : "err";
  li.textContent = message;
  $("log").prepend(li);
}

function act(label, fn) {
  try {
    const out = fn();
    log(label + (out ? `: ${out}` : ""));
    render();
    return out;
  } catch (err) {
    log(`${label} rejected — ${err.message}`, false);
    render();
    return null;
  }
}

function enterOne(name) {
  if (!draw) return log("Create a draw first", false);
  act(`Enter ${name}`, () => {
    if (draw.ticketPrice > 0) Sim.enter(draw, name);
    else Sim.enterFree(draw, name);
    return `entry #${draw.entries.length}`;
  });
}

function render() {
  const has = draw !== null;
  $("epoch-now").textContent = has ? String(draw.currentEpoch) : "–";

  // Entries list
  const list = $("entries-list");
  list.innerHTML = "";
  if (!has || draw.entries.length === 0) {
    list.innerHTML = "<li><em>none yet</em></li>";
  } else {
    draw.entries.forEach((e, i) => {
      const li = document.createElement("li");
      li.textContent = e + (draw.winner === e && draw.drawn ? " — winner" : "");
      li.value = i;
      list.appendChild(li);
    });
  }

  // Entrant buttons
  const box = $("enter-buttons");
  box.innerHTML = "";
  for (const name of DEMO_ENTRANTS) {
    const b = document.createElement("button");
    b.className = "secondary";
    b.textContent = `Enter ${name}`;
    b.disabled = !has || draw.entries.includes(name);
    b.onclick = () => enterOne(name);
    box.appendChild(b);
  }

  // Status + balances
  if (!has) {
    $("status").textContent = "No draw created yet.";
    $("balances").innerHTML = "";
  } else {
    $("status").textContent =
      `Prize pool: ${draw.pool} · Entries: ${draw.entries.length} · ` +
      `Ticket price: ${draw.ticketPrice} · Close epoch: ${draw.closeEpoch} · Draw epoch: ${draw.drawEpoch} · ` +
      `Drawn: ${draw.drawn} · Paid: ${draw.paid} · Refunded: ${draw.refunded}`;
    const accounts = [ORGANIZER, ...DEMO_ENTRANTS, ...draw.entries];
    const seen = [...new Set(accounts)];
    const rows = seen
      .map((a) => `<tr><td>${a}</td><td>${Sim.balanceOf(draw, a)}</td></tr>`)
      .join("");
    $("balances").innerHTML =
      `<table><tr><th>Account</th><th>Demo balance</th></tr>${rows}</table>`;
  }

  $("btn-claim").textContent =
    has && draw.drawn && draw.winner
      ? `Claim prize (as winner: ${draw.winner})`
      : "Claim prize";
  $("btn-claim").disabled = !has;
  $("btn-refund").disabled = !has;
  $("btn-draw").disabled = !has;
  $("btn-epoch1").disabled = !has;
  $("btn-epoch-close").disabled = !has;
  $("btn-epoch-draw").disabled = !has;
  $("btn-enter-custom").disabled = !has;
  $("btn-enter-all").disabled = !has;
}

$("btn-create").onclick = () => {
  const prize = Number($("in-prize").value);
  const ticketPrice = Number($("in-ticket").value);
  const closeEpoch = Number($("in-close").value);
  const drawEpoch = Number($("in-draw").value);
  try {
    draw = Sim.createDraw({
      organizer: ORGANIZER,
      prize,
      ticketPrice,
      closeEpoch,
      drawEpoch,
      seed: "undibukti-demo",
    });
    lastDrawResult = null;
    $("draw-result").hidden = true;
    $("log").innerHTML = "";
    log(
      `Draw created by ${ORGANIZER}: prize ${prize}, ticket ${ticketPrice} ` +
        `(${ticketPrice === 0 ? "free" : "paid"}), close epoch ${closeEpoch}, draw epoch ${drawEpoch}`
    );
  } catch (err) {
    draw = null;
    log(`Create rejected — ${err.message}`, false);
  }
  render();
};

$("btn-enter-custom").onclick = () => {
  const name = $("in-entrant").value.trim();
  if (!name) return log("Type an account name first", false);
  enterOne(name);
  $("in-entrant").value = "";
};

$("btn-enter-all").onclick = () => {
  for (const name of DEMO_ENTRANTS) {
    if (draw && !draw.entries.includes(name)) enterOne(name);
  }
};

$("btn-epoch1").onclick = () =>
  act("+1 epoch", () => {
    Sim.setEpoch(draw, draw.currentEpoch + 1);
    return `epoch ${draw.currentEpoch}`;
  });
$("btn-epoch-close").onclick = () =>
  act("Jump to close epoch", () => {
    Sim.setEpoch(draw, Math.max(draw.currentEpoch, draw.closeEpoch));
    return `epoch ${draw.currentEpoch}`;
  });
$("btn-epoch-draw").onclick = () =>
  act("Jump to draw epoch", () => {
    Sim.setEpoch(draw, Math.max(draw.currentEpoch, draw.drawEpoch));
    return `epoch ${draw.currentEpoch}`;
  });

$("btn-draw").onclick = () => {
  const result = act("Draw", () => {
    const r = Sim.draw(draw);
    return `winner index ${r.winnerIndex} (${r.winner})`;
  });
  if (result !== null && draw.drawn) {
    lastDrawResult = {
      epochHash: draw.epochHash,
      seed: Sim.seedFromHash(draw.epochHash),
      winnerIndex: draw.winnerIndex,
      winner: draw.winner,
    };
    $("res-hash").textContent = lastDrawResult.epochHash;
    $("res-seed").textContent = lastDrawResult.seed.toString();
    $("res-index").textContent = String(lastDrawResult.winnerIndex);
    $("res-winner").textContent = lastDrawResult.winner;
    $("draw-result").hidden = false;
  }
  render();
};

$("btn-claim").onclick = () => {
  if (!draw || !draw.winner) return log("Nothing to claim yet", false);
  act(`Claim as ${draw.winner}`, () => `${Sim.claim(draw, draw.winner)} paid out`);
};

$("btn-refund").onclick = () =>
  act(`Refund as ${ORGANIZER}`, () => `${Sim.refundAll(draw, ORGANIZER)} returned`);

// --- Verification section ---------------------------------------------------

$("btn-ver-load").onclick = () => {
  if (!draw || !draw.drawn) return log("Settle the demo draw first", false);
  $("ver-entries").value = draw.entries.join("\n");
  $("ver-hash").value = draw.epochHash;
  log("Verification form loaded from the demo draw");
};

$("btn-verify").onclick = () => {
  const entries = $("ver-entries")
    .value.split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  const hash = $("ver-hash").value.trim();
  const box = $("ver-result");
  box.hidden = false;
  try {
    const r = Sim.verifyWinner(entries, hash);
    let html =
      `Seed: <span class="mono">${r.seed.toString()}</span><br>` +
      `Winner index: <strong>${r.winnerIndex}</strong> of ${entries.length} entries<br>` +
      `Winner: <strong>${r.winner}</strong>`;
    if (lastDrawResult && Sim.normalizeHashHex(hash) === lastDrawResult.epochHash) {
      const same =
        r.winnerIndex === lastDrawResult.winnerIndex && r.winner === lastDrawResult.winner;
      html += `<br><span class="${same ? "match" : "mismatch"}">${
        same
          ? "Matches the demo draw result."
          : "Does NOT match the demo draw result."
      }</span>`;
    }
    box.innerHTML = html;
  } catch (err) {
    box.innerHTML = `<span class="mismatch">Cannot verify — ${err.message}</span>`;
  }
};

render();
