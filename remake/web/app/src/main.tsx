import "@fontsource/black-ops-one/latin.css";
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
import { getLobby, listServers } from "./data";
import { Home } from "./Home";
import { Lobby } from "./Lobby";
import { Servers } from "./Servers";

function Layout() {
  return (
    <MotionConfig reducedMotion="user">
      <div className="flex min-h-dvh flex-col">
        <header className="border-b-2 border-mud">
          <nav
            aria-label="Main"
            className="mx-auto flex max-w-6xl items-center justify-between gap-4 px-4 py-3"
          >
            <Link to="/" className="flex items-baseline gap-2">
              <span className="cartoon font-display text-2xl text-acorn">Conker</span>
              <span className="hidden font-stencil text-xs tracking-widest text-dust uppercase sm:inline">
                Bad Fur Day · Multiplayer
              </span>
            </Link>
            <Link
              to="/servers"
              className="font-stencil uppercase hover:text-acorn"
              activeProps={{ className: "text-acorn" }}
            >
              Servers
            </Link>
          </nav>
        </header>
        <Outlet />
        <footer className="mt-auto border-t-2 border-mud px-4 py-6 text-center text-xs text-dust">
          A fan remake of Conker's Bad Fur Day multiplayer in Rust and WebAssembly. Not affiliated
          with Rare, Microsoft or Nintendo.
        </footer>
      </div>
    </MotionConfig>
  );
}

const root = createRootRoute({
  component: Layout,
  notFoundComponent: () => (
    <main className="mx-auto w-full max-w-6xl px-4 py-16">
      <h1 className="cartoon font-display text-5xl">Nothing here</h1>
      <Link to="/" className="mt-4 inline-block text-acorn underline">
        Back to base
      </Link>
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
    // A fresh lobby (ready, chat) per server.
    remountDeps: ({ params }) => params.serverId,
    loader: async ({ params }) => {
      const lobby = await getLobby(params.serverId);
      if (!lobby) throw redirect({ to: "/servers" });
      return lobby;
    },
    component: Lobby,
  }),
] as const;

// Hash URLs (/#/servers, /#/lobby/local): any static server serves them, no fallback needed.
const router = createRouter({ routeTree: root.addChildren(routes), history: createHashHistory() });

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
