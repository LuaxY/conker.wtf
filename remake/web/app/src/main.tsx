import "@fontsource/black-ops-one/latin.css";
import "@fontsource/gloria-hallelujah/latin.css";
import "@fontsource/luckiest-guy/latin.css";
import "./index.css";
import {
  createHashHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Link,
  Outlet,
  redirect,
  RouterProvider,
} from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getServer, listServers } from "./data";
import { Home } from "./Home";
import { Lobby } from "./Lobby";
import { Servers } from "./Servers";
import { DANGER, Sprite } from "./ui";

function Layout() {
  return (
    <MotionConfig reducedMotion="user">
      <div className="flex min-h-dvh flex-col">
        <header className="border-b-3 border-black bg-black/40">
          <nav
            aria-label="Main"
            className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-3"
          >
            <Link
              to="/"
              className="group grid leading-none"
              aria-label="Conker's Bad Fur Day: home"
            >
              <span
                aria-hidden
                className="-mb-2 -ml-1 origin-left -rotate-6 font-hand text-2xl text-fur transition [-webkit-text-stroke:0.6px_currentColor] [text-shadow:2px_2px_0_#000] group-hover:-rotate-10 sm:text-3xl"
              >
                Conker's
              </span>
              <span
                aria-hidden
                className="cartoon cartoon-slate font-display text-lg whitespace-nowrap text-amber sm:text-2xl"
              >
                Bad Fur Day
              </span>
            </Link>
            <div className="flex items-center gap-2 sm:gap-3">
              <Link to="/" className="pill" activeOptions={{ exact: true }}>
                Home
              </Link>
              <Link to="/servers" className="pill">
                Servers
              </Link>
            </div>
          </nav>
        </header>
        <Outlet />
        <footer className="mt-auto border-t-3 border-black bg-black/60 px-4 py-5">
          <p className="mx-auto flex max-w-6xl flex-wrap items-center justify-center gap-x-3 gap-y-2 text-center text-xs text-dust">
            <span className="border-2 border-bone px-1.5 py-0.5 font-stencil tracking-widest text-bone">
              Advisory
            </span>
            A fan remake of Conker's Bad Fur Day multiplayer in Rust and WebAssembly. Not affiliated
            with Rare, Microsoft or Nintendo. No Rumble Pak needed.
          </p>
        </footer>
      </div>
    </MotionConfig>
  );
}

const root = createRootRoute({
  component: Layout,
  notFoundComponent: () => (
    <main className="mx-auto flex w-full max-w-6xl flex-wrap items-center gap-8 px-4 py-16">
      <Sprite
        src={DANGER}
        fallback={null}
        className="h-48 w-24 -rotate-3 drop-shadow-[6px_6px_0_#000]"
      />
      <div>
        <h1 className="cartoon font-display text-5xl text-tediz sm:text-6xl">You lose</h1>
        <p className="bubble mt-6 max-w-sm">Wrong turn, mate. There's nothing here.</p>
        <Link to="/" className="pill mt-8">
          Back to the pub
        </Link>
      </div>
    </main>
  ),
});

const routes = [
  createRoute({ getParentRoute: () => root, path: "/", component: Home }),
  createRoute({
    getParentRoute: () => root,
    path: "/servers",
    loader: listServers,
    component: Servers,
  }),
  createRoute({
    getParentRoute: () => root,
    path: "/lobby/$serverId",
    loader: async ({ params }) => {
      const server = await getServer(params.serverId);
      if (!server) throw redirect({ to: "/servers" });
      return server;
    },
    component: Lobby,
  }),
] as const;

// The router moves the scroll after every load, and a refresh (useRefresh's every 5 s, Run it, Kill)
// is a load too: only a new URL may move the page (to the top, or back where it was on Back).
let shown = "";
const moved = ({ location }: { location: { href: string } }) => {
  const changed = location.href !== shown;
  shown = location.href;
  return changed;
};

// Hash URLs (/#/servers, /#/lobby/1): any static server serves them, no fallback needed.
const router = createRouter({
  routeTree: root.addChildren(routes),
  history: createHashHistory(),
  scrollRestoration: moved,
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
