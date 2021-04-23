# Plutocradroid

*Plutocracy* - A system of government in which the wealthy rule.

For build & development options see **Developing** below.

## Commands

I suppose something is better than nothing, so here's something:

### Ping

```text
$ping
```

No arguments. Makes sure the bot is still there.

### Give

Structure:

```text
$give <user> <amount> <type>
```

`<user>` can be

* a username like `shelvacu`
* a nickname like `! ! Fre Sha Vaca Do`
* a tag like `shelvacu#8719`
* a user id like `165858230327574528` (obtained by enabling "developer mode", then right-clicking on a name/avatar and choosing "Copy ID")

`<amount>` is just a number, but must not contain anything that isn't a digit such as commas or periods.

* Yes: `1000`
* No: `1,000`
* No: `1.000`
* No: `1000.0`

`<type>` is either `pc` or `gen`/`gens`.

Examples:

```text
$give shelvacu 100 pc
$give shelvacu#8719 100gen
$give 165858230327574528 1 gens
```

### Force give

This is the same as `$give`, except that it DOES NOT CHECK IF THE USER EXISTS. While any name that can't be found still errors, if you specify a numerical user id, the bot will transfer regardless of whether that user exists. It is possible to "destroy" tokens this way, purposefully or accidentally.

```text
!!! DO NOT USE THIS UNLESS YOU KNOW WHAT YOU'RE DOING !!!
$force_give 1 100 pc
!!!
```

### Balances

```text
$balances
$b
```

Shows you how many generators and how much capital you have. Remember, except for motions the bot always responds in the same channel it receives the message in, so if you wish to keep your balances private, always run this command in DMs.

### Motion/Supermotion

```text
$motion <your text here>

$supermotion <your text here>
```

Calls a motion to be voted on. If `$motion` is used, the motion requires a simple majority for the bot to declare it as "passed". If `$supermotion` is used, the motion requires a supermajority, or greater than 2/3rds vote. According to the doc, any motion that "Changes to the core system, including: vote costs, bot behaviour, and creation and distribution of additional gens" must be passed with a 2/3rds vote, ie. with `$supermotion`

### Vote

```text
$vote <motion id> <direction> <amount>
$vote <motion id> <direction>
```

If you've already voted on the given motion at least once:

```text
$vote <motion id> <amount>
$vote <motion id>
```

This command casts votes on the given motion, costing capital. If the `amount` is not specified, it defaults to 1. If and only if you haven't voted on the motion before, you must specify the `direction`, such as `yes` or `no`.

Examples:

```text
$vote 123 yes
$vote 123 1000 yeah
$vote 123 fuck no
$vote 123
```

## Help & Version

```text
$help
```

Get help about what commands are available. Currently just links to this page.

```text
$version
```

Prints the current version and other details about the software.

## Reaction voting

Not the prettiest, but should still be more convenient than voting with the `$vote` command. On every motion, the bot reacts with certain emoji.

Clicking the "yes" or "no" emoji picks which direction you'd like to vote, however this is a "zero vote" and costs nothing (previously, this would cast one vote). This cannot be changed. Any problems are PM'd to you.

Clicking any of the numbers casts that number of votes. If you have not previously specified a direction in a previous `$vote` command or click on the "yes" or "no" emoji, this will not work. Any problems are PM'd to you.

You cannot retract votes. Un-reacting does nothing except allow you to react again, voting that many more times.

Generally, you'll want to click "yes" or "no" and then as many numbers as you like. The numbers are chosen such that any number of votes from 0 to 199 can be cast purely from the reactions, without un-reacting.

# Building

Plutocradroid requires rust nightly features. Install [`rustup`](https://rustup.rs/).

Install nightly rust

    rustup install nightly

Install dev dependencies (debian/ubuntu)

    apt install pkg-config build-essential libssl-dev libpq-dev

Build&run project

    RUN_BOT=1 cargo run
    RUN_WEB2=1 cargo run

Build in release mode (binary will be in `target/release/plutocradroid`)

    cargo build --release