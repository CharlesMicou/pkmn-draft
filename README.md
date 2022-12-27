# pkmn-draft

This is a simple application to let you draft competitive Pokemon sets with friends. It's (probably) hosted
here: [http://happylittleneurons.com](http://happylittleneurons.com)

If you're reading this, you probably care about the code, so here's a quick tour. Disclaimer: this is definitely
hackathon-grade nonsense.

### The draft server

The draft server is a rust application that lives in `src`. You can run it by running
`cargo run`. Setting the environment variable `PKMNDRAFT_PORT` will run the server on 0.0.0.0:PORT instead
of `localhost`.

`main.rs` does the following:

* Initialises a `lobby_manager` to handle drafting logic
* Starts a web server (defined in `routes.rs`) to handle requests

The lobby manager is single threaded. It's the receiving side of a MPSC channel: its thread will block until tasks are
queued up on the channel. The web request handling logic simply enqueues tasks on the channel, which contain (a) a
`LobbyManagerRequest`, (b) a future of a `LobbyManagerResponse` that the lobby manager will complete (allowing the web
server to block on its completion).

The routes the web server handles are as follows:

* The directory `www/static` is served under the path `/static`
* `GET new_draft` starts a new draft and retrieves a page with a link to the draft
* `GET join_draft/{draft_id}` retrieves a page with a form to join a draft
* `POST join_draft/{draft_id}` will submit a username to join a draft and then redirect to `draft/$draft_id/$player_id`
* `GET draft/{draft_id}/{player_id}` retrieves a page showing the current draft state from the view of a particular
  player
* `POST draft/{draft_id}/{player_id}` allows enqueueing a draft command (see below)

The following draft commands are supported:

* `poll()`: a long-poll. This will complete only when the game state of a lobby has changed
* `pick(item id)`: picks a draft item from a pack.
* `start_game()`: starts the draft from the lobby state

#### LobbyManager Internals

The LobbyManager manages several `DraftLobby`s. A `DraftLobby` wraps a `DraftState` (the logic for actually picking
items) with some extra information like player names and whether the lobby has started or not. Calls made by
the `LobbyManager` that mutate the `DraftLobby` (such as picking a pack, starting the lobby, or enforcing a draft
deadline) may return a 'draft deadline'. This is a time point in the future when that lobby wants to be scheduled to
trigger an enforcement event (makes sure that all players have made at least X picks by a certain time).
The `LobbyManager` achieves this by self-scheduling an item on its own task queue.

A draft lobby has a unique state from the perspective of each player that can be encoded in a `u64`: how many players
have joined the lobby (as players cannot leave), how many picks they have made so far, and whether or not they are
currently expected to be making a pick. This means that the 'poll' draft command can provide a current draft state, and
if the draft state changes on the server side we can allow the polling requests to complete.

The entire lobby manager system is pretty agnostic of _what_ is being drafted, it just knows that there is
a `DraftDatabase` somewhere that can map a draft item id (an integer) onto an HTML template.

#### Frontend

There's a pile of handlebars templates under `www/`. The draft frontend is in `www/draft_template.html`, and some pretty
sketchy logic in `routes.rs` pieces the templates together from draft internals. The templates are actually reloaded
from disk at every request, because this made iterating locally faster. It should be pretty straightforward to refactor
that out.

The CSS for the Pokemon sets is shamelessly lifted from PokePaste and Pokemon Showdown.

#### The draft sets

The draft sets live in `data/draft_sets.txt`. They're in standard Pokemon set format. However, the draft database
actually reads from `data/generated` and `data/generated_stats`, which contain HTML snippets that nicely format the
Pokemon sets for rendering. The way I've 'generated' these HTML snippets is hilariously hacky:

1. Paste the contents of `draft_sets.txt` into PokePaste. Save the resultant HTML file, and then feed it as an input
   to `scripts/pokepaste_parser.py`. This generates the `data/generated` files.
2. Paste the contents of `draft_sets.txt` into the Pokemon Showdown teambuilder. Save the webpage as HTML and feed
   into `scripts/yolo_parser.py`. This generates the `data/generated_stats` files.

#### Images not included

The rendered Pokemon sets will try to retrieve images from `static/assets`. These are not committed to this repository.
However, to assist you in finding images yourself, when you run the `scripts/pokepaste_parser.py` script you can supply
a folder which contains the assets you currently have, and will tell you which Pokemon you are missing assets for.