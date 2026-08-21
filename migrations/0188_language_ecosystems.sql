-- Where a language actually lives.
--
-- A trade tells somebody what to build. It does not tell them where the people
-- who do that work talk to each other, which list of libraries everybody reads
-- before writing their own, or which conference publishes its talks for free.
-- Somebody learning Rust from Cotonou has no way of discovering that
-- `this-week-in-rust` exists, and that gap costs more than any missing tutorial.
--
-- One row per language ecosystem, curated. Events are JSONB rather than a
-- second table: they are a list to display, never a thing to join on, and
-- their shape (online-only years, renamed conferences, regional editions)
-- changes faster than a schema should.

CREATE TABLE external_language_ecosystems (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Matches the `language` vocabulary used on challenge_templates and
    -- project_slices.code_languages, so a user's proven languages can be
    -- joined against this listing.
    language VARCHAR(40) NOT NULL UNIQUE,
    display_name VARCHAR(80) NOT NULL,
    -- The one link to open first. Usually the awesome-* list, which is the
    -- densest starting point in almost every ecosystem.
    community_url TEXT NOT NULL,
    -- Forums, chats, subreddits, newsletters. Ordered by usefulness, not by
    -- size: a slow forum where maintainers answer beats a busy chat.
    community_links JSONB NOT NULL DEFAULT '[]'::JSONB,
    -- [{"name", "url", "month", "scope"}]. `scope` is 'global', 'regional' or
    -- 'online' — an African contributor needs to know which of these they can
    -- attend without a visa.
    notable_events JSONB NOT NULL DEFAULT '[]'::JSONB,
    -- Said plainly, in the platform's own words: what this ecosystem is good
    -- at and who it suits. Not marketing copy lifted from the language's site.
    summary TEXT NOT NULL,
    is_curated BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 100,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT ecosystem_links_are_arrays CHECK (
        jsonb_typeof(community_links) = 'array'
        AND jsonb_typeof(notable_events) = 'array'
    ),
    CONSTRAINT ecosystem_community_url_is_a_url CHECK (
        community_url ~ '^https://'
    )
);

COMMENT ON TABLE external_language_ecosystems IS
    'Where each language community actually lives — lists, forums, events. '
    'Curated, because the value is in the selection: a link dump is what '
    'somebody already failed to navigate before arriving here.';

CREATE INDEX idx_language_ecosystems_curated
    ON external_language_ecosystems (sort_order, language)
    WHERE is_curated = TRUE;

CREATE OR REPLACE FUNCTION touch_language_ecosystems_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_language_ecosystems_updated_at
    BEFORE UPDATE ON external_language_ecosystems
    FOR EACH ROW EXECUTE FUNCTION touch_language_ecosystems_updated_at();

-- ═══════════════════════════════════════════════════════════════════
-- The initial curation
-- ═══════════════════════════════════════════════════════════════════
--
-- Ordered by how much of Skilluv's own work happens in them, then by how
-- open the ecosystem is to a first-time contributor. Months are the usual
-- ones; a year is deliberately absent, since a row that names 2026 is wrong
-- in 2027 and nobody will remember to fix it.

INSERT INTO external_language_ecosystems
    (language, display_name, community_url, community_links, notable_events, summary, sort_order)
VALUES
    ('rust', 'Rust',
     'https://github.com/rust-unofficial/awesome-rust',
     '[{"name": "This Week in Rust", "url": "https://this-week-in-rust.org"},
       {"name": "Rust Users Forum", "url": "https://users.rust-lang.org"},
       {"name": "r/rust", "url": "https://reddit.com/r/rust"},
       {"name": "Rust Zulip (development)", "url": "https://rust-lang.zulipchat.com"}]'::JSONB,
     '[{"name": "RustConf", "url": "https://rustconf.com", "month": "september", "scope": "global"},
       {"name": "EuroRust", "url": "https://eurorust.eu", "month": "october", "scope": "regional"},
       {"name": "Rust Nation UK", "url": "https://www.rustnationuk.com", "month": "march", "scope": "regional"},
       {"name": "Rust Global", "url": "https://rustfoundation.org", "month": "varies", "scope": "online"}]'::JSONB,
     'The ecosystem Skilluv itself is written in. Unusually welcoming to '
     'first contributions: most repositories label them, and reviews explain '
     'rather than reject. Slow to learn, and the review culture is why.',
     10),

    ('typescript', 'TypeScript / JavaScript',
     'https://github.com/sorrycc/awesome-javascript',
     '[{"name": "TypeScript Discord", "url": "https://discord.com/invite/typescript"},
       {"name": "r/typescript", "url": "https://reddit.com/r/typescript"},
       {"name": "Node.js Slack", "url": "https://www.nodeslackers.com"}]'::JSONB,
     '[{"name": "TSConf", "url": "https://tsconf.io", "month": "october", "scope": "global"},
       {"name": "NodeConf EU", "url": "https://www.nodeconf.eu", "month": "november", "scope": "regional"},
       {"name": "JSNation", "url": "https://jsnation.com", "month": "june", "scope": "online"}]'::JSONB,
     'The largest surface area of open issues anywhere, and the shortest path '
     'from reading code to shipping a fix. The cost is churn: a library that '
     'mattered two years ago may be unmaintained now.',
     20),

    ('python', 'Python',
     'https://github.com/vinta/awesome-python',
     '[{"name": "Python Discourse", "url": "https://discuss.python.org"},
       {"name": "r/Python", "url": "https://reddit.com/r/Python"},
       {"name": "Python Discord", "url": "https://pythondiscord.com"}]'::JSONB,
     '[{"name": "PyCon US", "url": "https://us.pycon.org", "month": "may", "scope": "global"},
       {"name": "EuroPython", "url": "https://europython.eu", "month": "july", "scope": "regional"},
       {"name": "PyCon Africa", "url": "https://africa.pycon.org", "month": "varies", "scope": "regional"},
       {"name": "PyData", "url": "https://pydata.org", "month": "varies", "scope": "online"}]'::JSONB,
     'Has the continent''s most active language association: PyCon Africa and '
     'the national PyCons are the rare events reachable without a visa. Data '
     'and tooling work dominates the open issues.',
     30),

    ('go', 'Go',
     'https://github.com/avelino/awesome-go',
     '[{"name": "Gophers Slack", "url": "https://invite.slack.golangbridge.org"},
       {"name": "r/golang", "url": "https://reddit.com/r/golang"},
       {"name": "Go Forum", "url": "https://forum.golangbridge.org"}]'::JSONB,
     '[{"name": "GopherCon", "url": "https://www.gophercon.com", "month": "july", "scope": "global"},
       {"name": "GopherCon Europe", "url": "https://gophercon.eu", "month": "june", "scope": "regional"},
       {"name": "GoLab", "url": "https://golab.io", "month": "november", "scope": "regional"}]'::JSONB,
     'Small language, large infrastructure ecosystem: most of what runs a '
     'cluster is written in it. Contributions tend to be short and reviewable, '
     'which suits a first merged pull request.',
     40),

    ('elixir', 'Elixir',
     'https://github.com/h4cc/awesome-elixir',
     '[{"name": "Elixir Forum", "url": "https://elixirforum.com"},
       {"name": "Elixir Slack", "url": "https://elixir-lang.slack.com"},
       {"name": "r/elixir", "url": "https://reddit.com/r/elixir"}]'::JSONB,
     '[{"name": "ElixirConf", "url": "https://elixirconf.com", "month": "august", "scope": "global"},
       {"name": "ElixirConf EU", "url": "https://www.elixirconf.eu", "month": "april", "scope": "regional"},
       {"name": "Code BEAM", "url": "https://codebeamamerica.com", "month": "varies", "scope": "online"}]'::JSONB,
     'Small enough that a regular contributor becomes a known name within a '
     'year — which is worth more than the same effort spent unnoticed in a '
     'crowded ecosystem.',
     50),

    ('c', 'C / systems',
     'https://github.com/oz123/awesome-c',
     '[{"name": "Linux Kernel Newbies", "url": "https://kernelnewbies.org"},
       {"name": "lore.kernel.org (mailing lists)", "url": "https://lore.kernel.org"},
       {"name": "r/C_Programming", "url": "https://reddit.com/r/C_Programming"}]'::JSONB,
     '[{"name": "Linux Plumbers Conference", "url": "https://lpc.events", "month": "september", "scope": "global"},
       {"name": "FOSDEM", "url": "https://fosdem.org", "month": "february", "scope": "regional"}]'::JSONB,
     'The contribution process is mailing lists and patch series, not pull '
     'requests, and that alone stops most people. Anybody who gets through it '
     'has proved something no tutorial can.',
     60),

    ('java', 'Java / JVM',
     'https://github.com/akullpp/awesome-java',
     '[{"name": "r/java", "url": "https://reddit.com/r/java"},
       {"name": "Foojay", "url": "https://foojay.io"},
       {"name": "Adoptium Slack", "url": "https://adoptium.net"}]'::JSONB,
     '[{"name": "JavaOne", "url": "https://www.oracle.com/javaone", "month": "march", "scope": "global"},
       {"name": "Devoxx", "url": "https://devoxx.com", "month": "varies", "scope": "regional"},
       {"name": "JCON", "url": "https://jcon.one", "month": "october", "scope": "online"}]'::JSONB,
     'Where most enterprise work on the continent actually happens, which '
     'makes proven Java contributions unusually easy to convert into paid '
     'work locally.',
     70),

    ('php', 'PHP',
     'https://github.com/ziadoz/awesome-php',
     '[{"name": "r/PHP", "url": "https://reddit.com/r/PHP"},
       {"name": "PHP Foundation", "url": "https://thephp.foundation"},
       {"name": "Laravel News", "url": "https://laravel-news.com"}]'::JSONB,
     '[{"name": "Laracon", "url": "https://laracon.net", "month": "varies", "scope": "global"},
       {"name": "PHP UK Conference", "url": "https://phpconference.co.uk", "month": "february", "scope": "regional"},
       {"name": "SymfonyCon", "url": "https://live.symfony.com", "month": "december", "scope": "regional"}]'::JSONB,
     'Dismissed more often than it is used, and it is used everywhere. A large '
     'share of the freelance market a Skilluv member can reach today is PHP '
     'maintenance work.',
     80),

    ('kotlin', 'Kotlin',
     'https://github.com/KotlinBy/awesome-kotlin',
     '[{"name": "Kotlin Slack", "url": "https://slack-chats.kotlinlang.org"},
       {"name": "r/Kotlin", "url": "https://reddit.com/r/Kotlin"},
       {"name": "Kotlin Discussions", "url": "https://discuss.kotlinlang.org"}]'::JSONB,
     '[{"name": "KotlinConf", "url": "https://kotlinconf.com", "month": "may", "scope": "global"},
       {"name": "droidcon", "url": "https://www.droidcon.com", "month": "varies", "scope": "regional"}]'::JSONB,
     'The default for Android, and droidcon holds editions in Lagos and '
     'Nairobi — among the few global conference series that comes to the '
     'continent rather than expecting travel out of it.',
     90),

    ('swift', 'Swift',
     'https://github.com/matteocrippa/awesome-swift',
     '[{"name": "Swift Forums", "url": "https://forums.swift.org"},
       {"name": "r/swift", "url": "https://reddit.com/r/swift"},
       {"name": "iOS Dev Weekly", "url": "https://iosdevweekly.com"}]'::JSONB,
     '[{"name": "WWDC", "url": "https://developer.apple.com/wwdc", "month": "june", "scope": "online"},
       {"name": "Swift Summit", "url": "https://www.swiftsummit.com", "month": "varies", "scope": "regional"}]'::JSONB,
     'Needs Apple hardware to do properly, which is a real barrier and worth '
     'saying rather than discovering after three weeks. Server-side Swift is '
     'the way in without it.',
     100),

    ('csharp', 'C# / .NET',
     'https://github.com/quozd/awesome-dotnet',
     '[{"name": "r/dotnet", "url": "https://reddit.com/r/dotnet"},
       {"name": ".NET Discord", "url": "https://aka.ms/dotnet-discord"},
       {"name": ".NET Blog", "url": "https://devblogs.microsoft.com/dotnet"}]'::JSONB,
     '[{"name": ".NET Conf", "url": "https://www.dotnetconf.net", "month": "november", "scope": "online"},
       {"name": "NDC Conferences", "url": "https://ndcconferences.com", "month": "varies", "scope": "regional"}]'::JSONB,
     'Fully open source since .NET Core, and the runtime repositories take '
     'outside contributions seriously. .NET Conf is free and online, which '
     'makes it one of the most accessible events in this table.',
     110),

    ('sql', 'PostgreSQL / SQL',
     'https://github.com/dhamaniasad/awesome-postgres',
     '[{"name": "pgsql-hackers mailing list", "url": "https://www.postgresql.org/list/pgsql-hackers"},
       {"name": "Postgres Slack", "url": "https://postgres-slack.herokuapp.com"},
       {"name": "Planet PostgreSQL", "url": "https://planet.postgresql.org"}]'::JSONB,
     '[{"name": "PGConf.dev", "url": "https://www.pgconf.dev", "month": "may", "scope": "global"},
       {"name": "PGConf EU", "url": "https://www.postgresql.eu", "month": "october", "scope": "regional"},
       {"name": "PGDay events", "url": "https://www.postgresql.org/about/events", "month": "varies", "scope": "regional"}]'::JSONB,
     'Contributing to the database itself is patch review over mailing lists '
     'and takes months. The extension ecosystem around it is where a first '
     'contribution is realistic.',
     120);
