#[macro_use] extern crate serenity;
#[macro_use] extern crate mysql;

extern crate dotenv;
extern crate typemap;
extern crate chrono_tz;
extern crate chrono;
extern crate reqwest;

use std::env;
use serenity::prelude::EventHandler;
use serenity::model::channel::GuildChannel;
use serenity::prelude::{Context, RwLock};
use dotenv::dotenv;
use typemap::Key;
use chrono_tz::Tz;
use chrono::prelude::*;
use std::sync::Arc;


struct MySQL;

impl Key for MySQL {
    type Value = mysql::Pool;
}


struct Handler;

impl EventHandler for Handler {
    fn channel_delete(&self, context: Context, channel: Arc<RwLock<GuildChannel>>) {
        let c = channel.read();
        let channel_id = c.id.as_u64();

        let data = context.data.lock();
        let my = data.get::<MySQL>().unwrap();

        my.prep_exec("DELETE FROM clocks WHERE channel = :c", params!{"c" => channel_id}).unwrap();
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
    let _ = message.channel_id.send_message(|m| {
        m.embed(|e| {
            e.title("Help")
            .description("
Go to our dashboard to add clock channels: **https://timezone.jellywx.com/**

`timezone personal <timezone name>` - Set your personal timezone, so others can check in on you.

`timezone check <user mention>` - Check the time in a user's timezone, if they set it with `timezone personal`.
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

Add clock channels from your browser: **https://timezone.jellywx.com/**

The bot can be summoned with a mention or using `timezone` as a prefix.

Do `timezone help` for more.
            ")
        })
    });
});
