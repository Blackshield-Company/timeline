const invoke = window.__TAURI__.core.invoke;

function showError(message) {
  const el = document.getElementById("error");
  el.hidden = !message;
  el.textContent = message || "";
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args || {});
  } catch (err) {
    const message = typeof err === "string" ? err : (err && err.message) || String(err);
    showError(message);
    throw err;
  }
}

function field(form, name) {
  return form.elements[name].value.trim();
}

function renderEvents(events) {
  const list = document.getElementById("events");
  list.replaceChildren();
  if (!events.length) {
    const empty = document.createElement("li");
    empty.textContent = "No events.";
    list.appendChild(empty);
    return;
  }
  for (const event of events) {
    const li = document.createElement("li");
    const when = document.createElement("span");
    when.className = "when";
    when.textContent = event.at;
    const body = document.createElement("span");
    body.textContent = event.description + " — " + event.source;
    li.append(when, body);
    list.appendChild(li);
  }
}

async function load() {
  showError("");
  const [events, path, lines] = await Promise.all([
    call("list_events"),
    call("store_path"),
    call("conflicts"),
  ]);
  renderEvents(events);
  document.getElementById("store-path").textContent = path;
  const conflicts = document.getElementById("conflicts");
  conflicts.replaceChildren();
  if (!lines.length) {
    const quiet = document.createElement("li");
    quiet.textContent = "No conflicts.";
    conflicts.appendChild(quiet);
    return;
  }
  for (const line of lines) {
    const li = document.createElement("li");
    li.textContent = line.text;
    conflicts.appendChild(li);
  }
}

document.addEventListener("submit", async (event) => {
  if (!event.target.matches("#new-event")) return;
  event.preventDefault();
  const form = event.target;
  const lat = field(form, "lat");
  const lon = field(form, "lon");
  const events = await call("add_event", {
    at: field(form, "at"),
    description: field(form, "description"),
    source: field(form, "source"),
    subject: field(form, "subject") || null,
    location: field(form, "location") || null,
    lat: lat ? Number(lat) : null,
    lon: lon ? Number(lon) : null,
  });
  form.reset();
  renderEvents(events);
  await load();
});

document.getElementById("export").addEventListener("click", async () => {
  const path = await call("export_markdown");
  document.getElementById("saved").textContent = "Wrote " + path;
});

load();
