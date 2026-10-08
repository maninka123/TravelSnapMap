// The real app (shell, views, dialogs) against the demo backend: what people see and what is sent to the backend.
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import App from "../App";
import { installDemoBackend, type DemoBackend, type DemoOptions } from "../dev/demoBackend";

function start(options: DemoOptions = {}): { backend: DemoBackend; user: ReturnType<typeof userEvent.setup> } {
  const backend = installDemoBackend(options);
  const user = userEvent.setup();
  render(<App />);
  return { backend, user };
}
const calls = (b: DemoBackend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);
const nav = (name: RegExp) => screen.getByRole("navigation", { name: "Main" }).querySelectorAll("button") &&
  within(screen.getByRole("navigation", { name: "Main" })).getByRole("button", { name });

describe("navigation", () => {
  it("opens Explore first and switches destinations from the sidebar and with ⌘ shortcuts", async () => {
    const { user } = start();
    expect(await screen.findByRole("combobox", { name: "Search saved places" })).toBeInTheDocument();
    await user.click(nav(/Import/));
    expect(await screen.findByRole("heading", { name: "Import" })).toBeInTheDocument();
    await user.keyboard("{Meta>}3{/Meta}");
    expect(await screen.findByRole("heading", { name: "My Trips" })).toBeInTheDocument();
    await user.keyboard("{Meta>},{/Meta}");
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("shows how many questions are waiting on the Import item", async () => {
    start();
    expect(await within(nav(/Import/)).findByLabelText("5 to review")).toBeInTheDocument();
  });
});

describe("Explore", () => {
  it("filters the library as you type and by status", async () => {
    const { user } = start();
    await user.click(await screen.findByRole("radio", { name: /Library/ }));
    expect(await screen.findByRole("button", { name: "Sydney Opera House" })).toBeInTheDocument();
    await user.type(screen.getByRole("combobox", { name: "Search saved places" }), "inari");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Sydney Opera House" })).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Fushimi Inari Taisha" })).toBeInTheDocument();
    await user.clear(screen.getByRole("combobox", { name: "Search saved places" }));
    await user.click(screen.getByRole("button", { name: /^Visited$/ }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Kiyomizu-dera" })).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Senso-ji" })).toBeInTheDocument();
  });

  it("says when nothing matches and offers to clear", async () => {
    const { user } = start();
    await user.click(await screen.findByRole("radio", { name: /Library/ }));
    await user.type(screen.getByRole("combobox", { name: "Search saved places" }), "qqqq");
    expect(await screen.findByText("No places match")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear search and filters" }));
    expect(await screen.findByRole("button", { name: "Sydney Opera House" })).toBeInTheDocument();
  });

  it("edits a place's status from its page", async () => {
    const { user, backend } = start();
    await user.click(await screen.findByRole("radio", { name: /Library/ }));
    await user.click(await screen.findByRole("button", { name: "Kiyomizu-dera" }));
    const dialog = await screen.findByRole("dialog", { name: "Details" });
    const status = await within(dialog).findByRole("combobox", { name: "Status" });
    await user.selectOptions(status, "visited");
    await waitFor(() => expect(calls(backend, "update_place").at(-1)?.args).toMatchObject({ id: "p1", field: "personalStatus", value: "visited" }));
  });
});

describe("first run and empty library", () => {
  it("walks through the welcome guide and lands on an empty map with next steps", async () => {
    localStorage.removeItem("onboarding.done");
    const { user } = start({ empty: true });
    expect(await screen.findByRole("heading", { name: "Your travel screenshots, on a map" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Skip for now" }));
    expect(await screen.findByText("Your travel map starts here")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Import screenshots or Reels/ })).toBeInTheDocument();
    expect(localStorage.getItem("onboarding.done")).toBe("1");
  });

  it("saves the AI key to the Keychain from the guide", async () => {
    localStorage.removeItem("onboarding.done");
    const { user, backend } = start({ empty: true, noApiKey: true });
    await user.click(await screen.findByRole("button", { name: /Get started/ }));
    await user.click(await screen.findByRole("button", { name: /Continue without Photos|Continue/ }));
    await user.click(await screen.findByRole("button", { name: /Continue/ }));
    await user.type(await screen.findByLabelText("DeepSeek API key"), "sk-test-123");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(calls(backend, "save_api_key").at(-1)?.args).toEqual({ key: "sk-test-123" }));
  });
});

describe("errors", () => {
  it("shows a readable error instead of a blank screen when the library can't load", async () => {
    start({ failWith: "database is locked" });
    expect(await screen.findByText(/Couldn't load your places: .*database is locked/)).toBeInTheDocument();
  });
});

describe("Import and Review", () => {
  it("starts reading new screenshots from Apple Photos", async () => {
    const { user, backend } = start();
    await user.click(nav(/Import/));
    await user.click(await screen.findByRole("button", { name: /Read 12 new/ }));
    await waitFor(() => expect(calls(backend, "start_processing").at(-1)?.args).toEqual({ mode: { kind: "scanNew" } }));
  });

  it("groups questions by reason and records a 'not a place' answer", async () => {
    const { user, backend } = start();
    await user.click(nav(/Import/));
    await user.click(await screen.findByRole("radio", { name: /Review/ }));
    const group = await screen.findByRole("region", { name: "Which place is this?" });
    expect(within(group).getByText(/Nothing goes on your map until you pick one/)).toBeInTheDocument();
    await user.click(within(group).getAllByRole("button", { name: "Not a place" })[0]);
    await waitFor(() => expect(calls(backend, "resolve_review").at(-1)?.args).toMatchObject({ id: "rv0", action: "dismiss" }));
  });

  it("shows 'All caught up' when nothing is waiting", async () => {
    const { user, backend } = start();
    backend.lib.reviews.forEach((r) => { r.isResolved = true; });
    await user.click(nav(/Import/));
    await user.click(await screen.findByRole("radio", { name: /Review/ }));
    expect(await screen.findByText("All caught up")).toBeInTheDocument();
  });
});

describe("My Trips", () => {
  it("creates a trip with dates and opens it", async () => {
    const { user, backend } = start();
    await user.click(nav(/My Trips/));
    await user.click((await screen.findAllByRole("button", { name: /New trip/ }))[0]);
    await user.type(screen.getByLabelText("Name"), "Sri Lanka in winter");
    await user.click(screen.getByRole("button", { name: "Create trip" }));
    await waitFor(() => expect(calls(backend, "create_trip").at(-1)?.args).toEqual({ name: "Sri Lanka in winter" }));
    expect(await screen.findByDisplayValue("Sri Lanka in winter")).toBeInTheDocument();
    expect(screen.getByText("No places in this trip yet")).toBeInTheDocument();
  });

  it("moves a stop to another day with the keyboard-friendly menu and saves the whole order", async () => {
    const { user, backend } = start();
    await user.click(nav(/My Trips/));
    await user.click(await screen.findByRole("button", { name: /Japan in spring/ }));
    await user.click(await screen.findByRole("button", { name: "Options for Shibuya Sky" }));
    await user.click(await screen.findByRole("menuitem", { name: "Move to Day 3" }));
    await waitFor(() => expect(calls(backend, "reorder_trip").length).toBe(1));
    const order = calls(backend, "reorder_trip")[0].args.order as { id: string; day: number | null }[];
    expect(order.find((o) => o.id === "tp2")?.day).toBe(3);
    expect(order.map((o) => o.day)).toEqual([...order.map((o) => o.day)].sort((a, b) => (a ?? 99) - (b ?? 99)));
  });
});

describe("Settings", () => {
  it("saves a setting as soon as it changes", async () => {
    const { user, backend } = start();
    await user.click(screen.getByRole("button", { name: /Settings/ }));
    await user.click(await screen.findByRole("switch", { name: "Read new screenshots automatically" }));
    await waitFor(() => expect((calls(backend, "save_settings").at(-1)?.args.config as { autoProcessNewScreenshots: boolean }).autoProcessNewScreenshots).toBe(true));
  });

  it("switches theme immediately", async () => {
    const { user } = start();
    await user.click(screen.getByRole("button", { name: /Settings/ }));
    await act(async () => { await user.click(await screen.findByRole("radio", { name: "Dark" })); });
    expect(document.documentElement.dataset.theme).toBe("dark");
    await user.click(screen.getByRole("radio", { name: "Match Mac" }));
    expect(document.documentElement.dataset.theme).toBeUndefined();
  });
});
