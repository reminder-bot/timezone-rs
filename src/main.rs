#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate chrono_tz;
extern crate chrono;
extern crate reqwest;

use std::env;
use serenity::prelude::EventHandler;
use serenity::model::gateway::{Game, Ready};
use serenity::model::channel::{ChannelType, PermissionOverwrite, PermissionOverwriteType};
use serenity::model::id::RoleId;
use serenity::prelude::Context;
use dotenv::dotenv;
use typemap::Key;
use serenity::model::permissions::Permissions;
use chrono_tz::Tz;
use chrono::prelude::*;


struct Globals;

impl Key for Globals {
    type Value = mysql::Pool;
}


struct Handler;

impl EventHandler for Handler {
    fn guild_create(&self, _context: Context, _guild: serenity::model::guild::Guild, _new: bool) {
        let guild_count = {
            let cache = serenity::CACHE.read();
            cache.all_guilds().len()
        };

        let c = reqwest::Client::new();
        c.post("https://discordbots.org/").header("Authorization", "token").send();
    }

    fn ready(&self, context: Context, _: Ready) {
        println!("Bot online!");

        context.set_game(Game::playing("@Bot o'clock help"));
    }
}


fn main() {
    dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("token");
    let sql_url = env::var("SQL_URL").expect("sql url");

    let mut client = serenity::client::Client::new(&token, Handler).unwrap();
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
        data.insert::<Globals>(my);
    }

    if let Err(e) = client.start() {
        println!("An error occured: {:?}", e);
    }
}


command!(new(context, message, args) {
    let g = match message.guild_id {
        Some(g) => g,

        None => return Ok(()),
    };

    let m = match message.member() {
        Some(m) => m,

        None => return Ok(()),
    };

    match m.permissions() {
        Ok(p) => {
            if !p.manage_guild() {
                let _ = message.reply("You must be a guild manager to perform this command.");
                return Ok(())
            }
        },

        Err(_) => return Ok(()),
    }

    let tz: String = match args.single::<String>() {
        Err(_) => {
            let _ = message.reply("Please supply a timezone for your new channel");
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
    if name.is_empty() {
        name = "🕒 %H:%M (%Z)";
    }

    let dt = Utc::now().with_timezone(&tz.parse::<Tz>().unwrap());

    match g.create_channel(dt.format(name).to_string().as_str(), ChannelType::Voice, None) {
        Ok(chan) => {
            let _ = message.channel_id.send_message(|m| {
                m.content("New channel created!")
            });

            let overwrite = PermissionOverwrite{
                allow: Permissions::empty(),
                deny: Permissions::CONNECT,
                kind: PermissionOverwriteType::Role(RoleId(*g.as_u64()))
            };

            match chan.create_permission(&overwrite) {
                Ok(_) => {},

                Err(_) => {
                    let _ = message.channel_id.send_message(|m| {
                        m.content("Channel was created, but permissions couldn't be applied.")
                    });
                }
            }

            {
                let mut data = context.data.lock();
                let mut mysql = data.get::<Globals>().unwrap();

                for mut stmt in mysql.prepare(r"INSERT INTO clocks (channel_id, timezone, name, guild_id) VALUES (:chan, :tz, :name, :guild)").into_iter() {
                    stmt.execute(params!{
                        "chan" => chan.id.as_u64(),
                        "tz" => &tz,
                        "name" => &name,
                        "guild" => g.as_u64(),
                    }).unwrap();
                }
            }
        },

        Err(e) => {
            let _ = message.channel_id.send_message(|m| {
                m.content(format!("Error creating channel: {:?}", e))
            });
        }
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
        let mut mysql = data.get::<Globals>().unwrap();


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
        let mut mysql = data.get::<Globals>().unwrap();

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
`timezone new <timezone name> [formatting]` - Create a new clock channel in your guild. You can customize the channel name as below:

```
Available inputs: %H (hours), %M (minutes), %Z (timezone), %d (day), %p (AM/PM), %A (day name), %I (12 hour clock)

Example:
    %H o'clock on the %dth
Displays:
    {}

Default Value:
    🕒 %H:%M (%Z)

More inputs can be found here: http://strftime.org/
```

`timezone personal <timezone name>` - Set your personal timezone, so others can check in on you.

`timezone check <user mention>` - Check the time in a user's timezone, if they set it with `timezone personal`.

`timezone delete [id]` - Delete timezone channels. Without arguments, will clean up channels manually deleted or delete a channel you are connected to in voice.
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
