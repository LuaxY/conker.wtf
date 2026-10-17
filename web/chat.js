// Text chat (conker_client::chat): the client calls window.conkerChat's open(team) when a chat key
// goes down (T everyone, Y the team; the box lets go of the pointer lock) and show(name, team, text)
// for each line the server relays. Enter sends the box's line to the client's chat_done, Esc or a
// click elsewhere closes it with none; either way the keys go back to the game's canvas. The list
// keeps the last LINES lines, each shown SHOW ms (all while the box is open); everyone's white, the
// team's light blue. The lines go in as text, never as HTML.

const LINES = 8;
const SHOW = 20_000;

/** Sets window.conkerChat; `done` takes the box's line, or nothing (the client's chat_done). */
export function chat(done) {
  const panel = document.getElementById("chat");
  const list = document.getElementById("chat-lines");
  const box = document.getElementById("chat-box");
  const [to, input] = box.children;
  const canvas = document.getElementById("game");

  const close = (line) => {
    if (box.hidden) return;
    box.hidden = true;
    panel.classList.remove("open");
    input.value = "";
    done(line);
    canvas.focus();
  };
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") close(input.value);
    else if (e.key === "Escape") close();
    else return;
    e.preventDefault();
  });
  input.addEventListener("blur", () => close());

  window.conkerChat = {
    open(team) {
      to.textContent = team ? "(Team)" : "(All)";
      box.classList.toggle("team", team);
      box.hidden = false;
      panel.classList.add("open");
      // The client's pointer code opens no menu for this loss; Enter takes the lock back.
      if (document.pointerLockElement) document.exitPointerLock();
      input.focus();
    },
    show(name, team, text) {
      const line = document.createElement("li");
      const who = document.createElement("b");
      who.textContent = `${team ? "(Team) " : ""}${name}: `;
      line.append(who, text);
      line.classList.toggle("team", team);
      list.append(line);
      while (list.children.length > LINES) list.firstChild.remove();
      setTimeout(() => line.classList.add("old"), SHOW);
    },
  };
}
