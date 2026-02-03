use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_user")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub username: String,
    pub password_hash: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub created: Option<DateTimeUtc>,
    pub updated: Option<DateTimeUtc>,
    pub role: Option<String>,
    pub avatar_url: Option<String>,
    pub last_clicked_mentioned: Option<DateTimeUtc>,
    pub default_visibility: Option<String>,
    pub default_enable_comment: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
