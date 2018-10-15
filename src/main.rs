#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate chrono_tz;

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


struct Globals;

impl Key for Globals {
    type Value = mysql::Pool;
}


struct Handler;

impl EventHandler for Handler {
    fn ready(&self, context: Context, _: Ready) {
        println!("Bot online!");

        context.set_game(Game::playing("@Bot o'clock help"));
    }
}


fn main() {
    dotenv().ok();

    let token = env::var("DISCORD_TOKEN").expect("token");

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
    );

    let my = mysql::Pool::new("mysql://root:testpassword@localhost/timezone").unwrap();

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
                let _ = message.reply("Timezone couldn't be parsed. Please try again");
                return Ok(())
            },

            Ok(_) => p
        },
    };
    let name = args.rest();

    match g.create_channel(&name, ChannelType::Voice, None) {
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

        Err(_) => {
            let _ = message.channel_id.send_message(|m| {
                m.content(format!("Error creating channel"))
            });
        }
    }
});

command!(help(_context, message) {
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Help")
            .description("
`timezone new <timezone name> [channel name]` - Create a new clock channel in your guild. You can customize the channel name as below:

```
Available inputs: %H (hours), %M (minutes), %Z (timezone), %d (day), %p (AM/PM), %A (day name), %I (12 hour clock)

Example:
    %H o'clock on the %dth
Displays:
    {hours} o'clock on the {days}th

Default Value:
    🕒 %H:%M (%Z)

More inputs can be found here: http://strftime.org/
```

`timezone personal <timezone name>` - Set your personal timezone, so others can check in on you.

`timezone check <user mention>` - Check the time in a user's timezone, if they set it with `timezone personal`.

`timezone space <timezone> [formatting]` - Place a timezone as a message in chat.

`timezone delete [id]` - Delete timezone channels. Without arguments, will clean up channels manually deleted or delete a channel you are connected to in voice.
            ")
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
