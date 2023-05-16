# search-tui

flexible tui application to search for stuff.

## Usage

to run the program, one must pipe its configuration into stdin of
the program (weird behavior i know).

```bash
cat aodb.json | search-tui
```

its configuration is a json file, and it's fairly simple in the
current state.

- `query_command` specifies the command to execute when the program
want to search for entries. it is a json object, with properties
`executable` and `args`. these properties are templates, which have
template variables `{query}` and `{query_escaped}`. the process stdout
would then be captured and parsed into some json object that looks like:

```jsonc
// comments are not allowed, this is jsonc for documentation purposes
{
    "results": [
        {
            // the identifier of the entry
            // the selected entry would have its identifier dumped into
            // stderr when the program exits
            "identifier": "id1",
            // the title of the entry
            "title": "entry 1",
            // the confidence of the search
            // the program expected the search engine to sort the results
            // array by this value in descending order
            "confidence": 0.92324132312,
            // the properties above can be used as template variables in
            // the `display_template`
        },
        // more entries here...
    ]
}
```

- `timeout_millis` is the timeout between each queries, this is used to
rate limit heavy operations. the unit is in milliseconds, and floating
point numbers are not allowed.

- `display_template` is the template used to display the search results
in the TUI. supported template variables are `{identifier}`, `{title}`,
`{confidence}`, `{index}`, `{display_index}`, `{one_based_index}` and
`{one_based_display_index}`.

templates are heavily used in the program configuration, and to reference
a variable `a`, one can use the syntax `{a}`. internally, the program uses
[TinyTemplate](https://github.com/bheisler/TinyTemplate), and there are
several additional features that one can use like conditionals, etc. to
escape the sequence `{blabla}`, one simply add a backslash character like
so: `\{blabla}` (which should be `"\\{blabla}"` in the json config).

there is an example `tsv-search.json` that show how to configurate this
program to query using [tsv-search](https://github.com/ngoduyanh/tsv-search).
(it needs some dependencies: `curl`, `jq` and `sh`, maybe it could be run
in git bash for windows idk).

## Original Usage

this was designed as a search tui to make the
[nrs](https://github.com/ngoduyanh/nrs-impl) entry ranking process much less
painful, it was intended to be invoked from some nrs rank script (which 
explains its stdout/stderr nature). bla bla unix philosophy...

## License

gplv3 cuck license
