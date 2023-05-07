use std::{collections::HashMap, thread, time};

#[derive(Debug, PartialEq, Eq)]
enum Type {
    Gold,
    Silver,
    Bronze,
}

#[derive(Debug)]
struct Medal {
    r#type: Type,
    country: String,
}

// Determines the current list of (athletics) medals as advertised by olympics.com.
fn fetch_medals() -> Result<Vec<Medal>, Box<dyn std::error::Error>> {
    // From: https://olympics.com/en/olympic-games/tokyo-2020/results/athletics
    let url = "https://path.to.file/athletics.json";
    let json: serde_json::Value = reqwest::blocking::get(url)?.json()?;
    let mut medals = vec![];
    for event in json["pageProps"]["gameDiscipline"]["events"]
        .as_array()
        .unwrap()
    {
        for award in event["awards"].as_array().unwrap() {
            let r#type = match award["medalType"].as_str().unwrap() {
                "GOLD" => Type::Gold,
                "SILVER" => Type::Silver,
                "BRONZE" => Type::Bronze,
                _ => panic!(),
            };
            let country = if !award["participant"]["countryObject"].is_object() {
                award["participant"]["title"].as_str().unwrap()
            } else {
                award["participant"]["countryObject"]["name"]
                    .as_str()
                    .unwrap()
            };
            let medal = Medal {
                r#type,
                country: country.to_string(),
            };
            medals.push(medal);
        }
    }
    Ok(medals)
}

// Returns a list of (country, #gold, #silver, #bronze) tuples
// ranked by medal count.
fn create_table(medals: &Vec<Medal>) -> Vec<(String, usize, usize, usize)> {
    // Collect all medals a country has won
    let mut by_country = HashMap::new();
    for medal in medals {
        by_country
            .entry(medal.country.clone())
            .or_insert(Vec::new())
            .push(medal);
    }

    // Collect the number of gold/silver/bronze for each country
    let mut countries = vec![];
    for (country, country_medals) in by_country.into_iter() {
        countries.push((
            country.into(),
            country_medals
                .iter()
                .filter(|x| x.r#type == Type::Gold)
                .count(),
            country_medals
                .iter()
                .filter(|x| x.r#type == Type::Silver)
                .count(),
            country_medals
                .iter()
                .filter(|x| x.r#type == Type::Bronze)
                .count(),
        ));
    }

    // Sort by reverse gold/silver/bronze medal count
    countries.sort_by_key(|elem| (elem.1, elem.2, elem.3));
    countries.into_iter().rev().collect()
}

fn main() {
    let mut last_top5 = None;

    loop {
        let medals = fetch_medals().unwrap();
        let table = create_table(&medals);
        let top5: Option<Vec<String>> = Some(table.iter().take(5).map(|e| e.0.clone()).collect());
        if top5 != last_top5 {
            println!("{:#?}", &top5);
        }
        last_top5 = top5;
        thread::sleep(time::Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use core::time;
    use reqwest::Url;
    use serde::Deserialize;
    use serde_json::Value;
    use std::collections::HashMap;
    use std::ops::Index;
    use std::ops::IndexMut;
    use std::str::FromStr;
    use std::thread;

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    enum Class {
        #[serde(rename = "GOLD")]
        Gold,
        #[serde(rename = "SILVER")]
        Silver,
        #[serde(rename = "BRONZE")]
        Bronze,
    }

    #[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct MedalCount {
        g: usize,
        s: usize,
        b: usize,
    }

    impl Index<Class> for MedalCount {
        type Output = usize;

        fn index(&self, class: Class) -> &Self::Output {
            match class {
                Class::Gold => &self.g,
                Class::Silver => &self.s,
                Class::Bronze => &self.b,
            }
        }
    }

    impl IndexMut<Class> for MedalCount {
        fn index_mut(&mut self, class: Class) -> &mut Self::Output {
            match class {
                Class::Gold => &mut self.g,
                Class::Silver => &mut self.s,
                Class::Bronze => &mut self.b,
            }
        }
    }

    struct AthleticsDb(AthleticsDbInner);

    struct AthleticsDbInner {
        json: serde_json::Value,
    }

    fn get_class_country_tuple<'a>(values: &'a Vec<Value>) -> Vec<(Class, String)> {
        let get_country = |participant: &'a serde_json::Value| -> &'a serde_json::Value {
            let mut country_key = "countryObject";
            if !participant[country_key].is_object() {
                country_key = "country";
            }

            &participant[country_key]["name"]
        };

        let to_tuple = |v: &'a Value| {
            let class = serde_json::from_value::<Class>(v["medalType"].to_owned()).unwrap();
            let country = get_country(&v["participant"]).as_str().unwrap().to_string();
            (class, country)
        };

        values.iter().map(to_tuple).collect()
    }

    impl AthleticsDb {
        pub fn from_url(url: Url) -> Result<Self, Box<dyn std::error::Error>> {
            let json: serde_json::Value = reqwest::blocking::get(url)?.json()?;
            Ok(AthleticsDb(AthleticsDbInner { json }))
        }

        pub fn get_medals_per_country(&self) -> Result<Projection, Box<dyn std::error::Error>> {
            let to_medal_country_tuple =
                |v: &serde_json::Value| get_class_country_tuple(v["awards"].as_array().unwrap());

            let group_by_country = |mut acc: HashMap<_, _>, x| {
                let (class, country): (Class, String) = x;
                acc.entry(country).or_insert(MedalCount::default())[class] += 1;
                acc
            };

            let events = self.0.json["pageProps"]["gameDiscipline"]["events"]
                .as_array()
                .unwrap();

            let mut medals_per_country = events
                .into_iter()
                .flat_map(to_medal_country_tuple)
                .fold(HashMap::new(), group_by_country)
                .into_iter()
                .collect::<Vec<(_, _)>>();

            medals_per_country.sort_by(|a, b| b.1.cmp(&a.1));

            Ok(Projection(medals_per_country))
        }
    }

    #[derive(Eq, PartialEq)]
    struct Projection(Vec<(String, MedalCount)>);

    impl Projection {
        fn empty() -> Self {
            Self(vec![])
        }

        fn take(self, n: usize) -> Self {
            Self(self.0.into_iter().take(n).collect())
        }

        fn get(&self) -> &Vec<(String, MedalCount)> {
            &self.0
        }
    }

    #[test]
    fn test_dummy() {
        assert_eq!(1, 1);
    }

    #[test]
    fn new_test() -> Result<(), Box<dyn std::error::Error>> {
        let url = "https://path.to.json/athletics.json";
        let db = AthleticsDb::from_url(Url::from_str(url)?)?;

        let mut last_top5 = Projection::empty();
        loop {
            let medals_per_country = db.get_medals_per_country()?;
            let top5 = medals_per_country.take(5);

            if top5 != last_top5 {
                for e in top5.get() {
                    println!("{} {:?}", e.0, e.1);
                }

                last_top5 = top5;
            }
            thread::sleep(time::Duration::from_secs(2));
        }
    }
}
