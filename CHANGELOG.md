## 1.5.2

* Bump auction timer from 48hr to 96hr per [motion#4224](https://mas.consortium.chat/motions/4224).

## 1.5.1

* Display the text of motions when the finish, even if they failed
* Add `bowlercaptain` to the known user list

## 1.5.0

* Added Submotions, requiring a one-third consensus to pass.
* Added a Discord command `$submotion` for calling Submotions.
* Added subcommands: `bot`, `web`, `worker`, and `fix_transactions`
* Removed env mode flags

## 1.3.1

* Added users to known user list

## 1.3.0

* Added meta tags for fancy embeds when you paste a motion or auction MAS link in Discord, Signal, Twitter, and others.

  To work around issues with embed data being cached, "live" data (current bid, number of votes) is only shown if `?cb=` (cache buster) is added to the url. You should type a bunch of random characters, like https://mas.consortium.chat/motions/13?cb=rf34ui218o9fhr78q9. With no `?cb`, only static data is shown unless the motion/auction has finished.

  The same applies to shortlinks, like https://m-a-s.cc/13?cb=fn738219ofjked.

  **This only changes embeds. The page will look the same regardless of `cb` when you visit in a browser.**
* Added a few other tags in `<head>` for things like canonical url and theme color
* Dark mode added to MAS. Site responds to system settings only for now.
* Added the CONsortium logo to the top of every page and as the favicon.
* The home page no longer lists motions, it simply says "Welcome to CONsortium MAS.", and motions are listed at `/motions`.

## 1.2.3

Increase the number of gens offered on auction every week from 1 to 10 per motion#2373

## 1.2.1 - 1.2.2

Mistakes were made

## 1.2.0

* You can now transfer fungibles from the weeb interface
* Added `Anthony` to the list of known names, will show the name rather than a discord id next to vote counts.

## The Past

Stuff happened
