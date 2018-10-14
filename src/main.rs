#[macro_use] extern crate serenity;

extern crate dotenv;
extern crate typemap;
extern crate mysql;

use std::env;
use serenity::prelude::EventHandler;
use serenity::model::gateway::{Game, Ready};
use serenity::prelude::Context;
use dotenv::dotenv;
use typemap::Key;


struct Globals;

impl Key for Globals {
    type Value = mysql::Conn;
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
        .configure(|c| c.prefix("?t"))

        .cmd("help", help)
        .cmd("invite", info)
        .cmd("info", info)
    );

    let mut my = mysql::Conn::new("mysql://root:testpassword@localhost/timezone").unwrap();

    {
        let mut data = client.data.lock();
        data.insert::<Globals>(my);
    }

    if let Err(e) = client.start() {
        println!("An error occured: {:?}", e);
    }
}

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
