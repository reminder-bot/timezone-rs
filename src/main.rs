#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate chrono_tz;
extern crate chrono;
extern crate reqwest;
extern crate threadpool;

use std::env;
use serenity::prelude::EventHandler;
use serenity::model::gateway::{Game, Ready};
use serenity::model::channel::{GuildChannel, ChannelType, PermissionOverwrite, PermissionOverwriteType};
use serenity::model::id::RoleId;
use serenity::prelude::{Context, RwLock};
use dotenv::dotenv;
use typemap::Key;
use serenity::model::permissions::Permissions;
use chrono_tz::Tz;
use chrono::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;
use threadpool::ThreadPool;


struct MySQL;

impl Key for MySQL {
    type Value = mysql::Pool;
}


struct Handler;

impl EventHandler for Handler {
    fn guild_create(&self, _context: Context, _guild: serenity::model::guild::Guild, is_new: bool) {
        if is_new {
            send();
        }
    }

    fn guild_delete(&self, _context: Context, _guild: serenity::model::guild::PartialGuild, _full: Option<std::sync::Arc<serenity::prelude::RwLock<serenity::model::guild::Guild>>>) {
        send();
    }

    fn channel_delete(&self, context: Context, channel: Arc<RwLock<GuildChannel>>) {
        let c = channel.read();
        let channel_id = c.id.as_u64();

        let data = context.data.lock();
        let my = data.get::<MySQL>().unwrap();

        my.prep_exec("DELETE FROM clocks WHERE channel = :c", params!{"c" => channel_id}).unwrap();
    }

    fn ready(&self, context: Context, _: Ready) {
        println!("Bot online!");

        context.set_game(Game::playing("@Bot o'clock help"));
    }
}


fn send() {
    let guild_count = {
        let cache = serenity::CACHE.read();
        cache.all_guilds().len()
    };

    let c = reqwest::Client::new();
    let mut m = HashMap::new();
    m.insert("server_count", guild_count);

    c.post("https://discordbots.org/api/bots/stats").header("Authorization", env::var("DBL_TOKEN").unwrap()).header("Content-Type", "application/json").json(&m).send().unwrap();
}


fn main() {
    dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("token");
    let sql_url = env::var("SQL_URL").expect("sql url");
    let threadpool = ThreadPool::new(20);

    let mut client = serenity::client::Client::new(&token, Handler).unwrap();
    client.threadpool = threadpool;
    client.with_framework(serenity::framework::standard::StandardFramework::new()
        .configure(|c| c
            .prefix("timezone ")
            .on_mention(true)
        )

        .cmd("help", help)
        .cmd("invite", info)
        .cmd("info", info)
        .cmd("new", new)
        .cmd("personal", personal)
        .cmd("check", check)
    );

    let my = mysql::Pool::new(sql_url).unwrap();

    {
        let mut data = client.data.lock();
        data.insert::<MySQL>(my);
    }

    if let Err(e) = client.start_autosharded() {
        println!("An error occured: {:?}", e);
    }
}


command!(new(context, message, args) {
    match message.member() {
        Some(m) => {
            match m.permissions() {
                Ok(p) => {
                    if !p.manage_guild() {
                        let _ = message.reply("You must be a guild manager to perform this command.");
                        return Ok(())
                    }
                },

                Err(_) => return Ok(()),
            }

            let mut data = context.data.lock();
            let mut mysql = data.get::<MySQL>().unwrap();

            let mut q = mysql.prep_exec("SELECT COUNT(*) FROM clocks WHERE guild = :g", params!{"g" => message.guild_id.unwrap().as_u64()}).unwrap();

            let v = mysql::from_row::<(u32)>(q.next().unwrap().unwrap());

            let max = env::var("MAX_CHANNELS").unwrap().parse::<u32>().unwrap();

            if v >= max {
                let _ = message.channel_id.say(&format!("Guilds are allowed a maximum of {} clocks each", max));
            }

            else {
                let tz: String = match args.single::<String>() {
                    Err(_) => {
                        let _ = message.reply("Please supply a timezone for your new clock");
                        return Ok(())
                    },

                    Ok(p) => match p.parse::<Tz>() {
                        Err(_) => {
                            let _ = message.reply("Timezone couldn't be parsed. Please try again. A list of timezones is available here: https://gist.github.com/JellyWX/913dfc8b63d45192ad6cb54c829324ee");
                            return Ok(())
                        },

                        Ok(_) => p
                    },
                };

                let mut name = args.rest();

                if !name.contains("%") || name.len() > 64 {
                    if name.starts_with("preset:") {
                        name = match name {
                            "preset:24" => "🕒 %H:%M (%Z)",

                            "preset:24:plain" => "%H:%M  %Z",

                            "preset:24:minimal" => "%H:%M",

                            "preset:12" => "🕒 %I:%M %p (%Z)",

                            "preset:12:plain" => "%I:%M %p  %Z",

                            "preset:12:minimal" => "%I:%M %p",

                            "preset:day" => "%A",

                            _ => "🕒 %H:%M (%Z)",
                        };
                    }
                    else {
                        name = "🕒 %H:%M (%Z)";
                    }
                }

                let dt = Utc::now().with_timezone(&tz.parse::<Tz>().unwrap());

                let g = m.guild_id;

                match g.create_channel(dt.format(name).to_string().as_str(), ChannelType::Voice, None) {
                    Ok(chan) => {
                        let _ = message.channel_id.say("New channel created!");

                        let overwrite = PermissionOverwrite{
                            allow: Permissions::empty(),
                            deny: Permissions::CONNECT,
                            kind: PermissionOverwriteType::Role(RoleId(*g.as_u64()))
                        };

                        let _ = chan.create_permission(&overwrite);

                        mysql.prep_exec(r"INSERT INTO clocks (channel, timezone, name, guild) VALUES (:chan, :tz, :name, :guild)", params!{
                                "chan" => chan.id.as_u64(),
                                "tz" => tz,
                                "name" => name,
                                "guild" => message.guild_id.unwrap().as_u64(),
                            }).unwrap();    
                    },

                    Err(_) => {
                        let _ = message.channel_id.say("Error creating channel. Please ensure I have admin");
                    }
                }
            }
        },

        None => {},
    }
});


command!(personal(context, message, args) {

    let arg = args.single::<String>().unwrap();
    let tz: Tz = match arg.parse() {
        Err(_) => {
            let _ = message.reply("Please provide a valid timezone as an argument. All timezones can be viewed here: https://gist.github.com/JellyWX/913dfc8b63d45192ad6cb54c829324ee");
            return Ok(())
        },

        Ok(t) => t
    };

    let dt = Utc::now().with_timezone(&tz);
    let _ = message.reply(&format!("Your current time should be {}", dt.format("%H:%M")));

    {
        let mut data = context.data.lock();
        let mut mysql = data.get::<MySQL>().unwrap();


        let cq = mysql.prep_exec("SELECT COUNT(*) FROM users WHERE id = :id", params!{"id" => message.author.id.as_u64()}).unwrap()
            .into_iter()
            .next().unwrap();
        let count = mysql::from_row::<i32>(cq.unwrap());

        if count > 0 {
            mysql.prep_exec("UPDATE users SET timezone = :tz WHERE id = :id", params!{
                "id" => message.author.id.as_u64(),
                "tz" => &arg
            })
            .unwrap();
        }
        else {
            mysql.prep_exec(
                r"INSERT INTO users (id, timezone) VALUES (:id, :tz)",
                params!{
                    "id" => message.author.id.as_u64(),
                    "tz" => &arg
                })
            .unwrap();
        }
    }
});


command!(check(context, message) {
    if message.mentions.len() == 1 {
        let mut data = context.data.lock();
        let mut mysql = data.get::<MySQL>().unwrap();

        for res in mysql.prep_exec("SELECT timezone FROM users WHERE id = :id", params!{"id" => message.mentions.first().unwrap().id.as_u64()}).unwrap() {
            let tz = mysql::from_row::<String>(res.unwrap());

            let r: Tz = tz.parse().unwrap();
            let dt = Utc::now().with_timezone(&r);

            let _ = message.channel_id.send_message(|m| {
                m.content(format!("{}'s current time is `{}`", message.mentions.first().unwrap().name, dt.format("%H:%M")))
            });
        }
    }
    else {
        let _ = message.reply("Please mention the user you wish to check the timezone of.");
    }
});


command!(help(_context, message) {
    let dt = Utc::now();

    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Help")
            .description(
                format!("
`timezone new <timezone name> [formatting]` - Create a new clock channel in your guild. You can customize the channel name using a preset or as in the available inputs section (advanced).


**Available presets:**
- `preset:24` - shows the 24 hour time
- `preset:24:plain` - shows the time without emoji
- `preset:24:minimal` - shows the time without emoji or timezone
- `preset:12` - shows the 12 hour clock time
- `preset:12:plain` - shows the time without emoji
- `preset:12:minimal` - shows the time without emoji or timezone
- `preset:day` - shows the day


```
Available inputs: %H (hours), %M (minutes), %Z (timezone), %d (day), %p (AM/PM), %A (day name), %I (12 hour clock)

Example:
    %H o'clock on the %dth
Displays:
    {}

Default Value:
    🕒 %H:%M (%Z)

```
*More inputs can be found here: https://strftime.ninja/*

`timezone personal <timezone name>` - Set your personal timezone, so others can check in on you.

`timezone check <user mention>` - Check the time in a user's timezone, if they set it with `timezone personal`.
            ", dt.format("%H o'clock on the %dth"))
        )
        })
    });
});


command!(info(_context, message) {
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Info")
            .description("
Invite me: https://discordapp.com/oauth2/authorize?client_id=485424873863118848&scope=bot&permissions=8

Bot o'clock is a part of the Fusion Network:
https://discordbots.org/servers/366542432671760396

It well accompanies Reminder Bot by @JellyWX:
https://discordbots.org/bot/349920059549941761

The bot can be summoned with a mention or using `timezone` as a prefix.

Do `timezone help` for more.
            ")
        })
    });
});
