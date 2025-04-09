use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "person")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, default = "uuid_generate_v4()")]
    pub id: uuid::Uuid,

    #[sea_orm(column_type = "Text")]
    pub name: String,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_id: String, // e.g. 'google', 'github', etc.

    #[sea_orm(column_type = "Text")]
    pub auth_provider_user_id: String,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_access_token: String,

    #[sea_orm(column_type = "Text")]
    pub auth_provider_refresh_token: String,

    #[sea_orm(column_type = "Text")]
    pub email: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::person_organization::Entity")]
    PersonOrganization,

    #[sea_orm(has_one = "super::person_auth_google::Entity")]
    PersonAuthGoogle,
}

impl Related<super::person_organization::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PersonOrganization.def()
    }
}

impl Related<super::organization::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PersonOrganization.def()
    }
}

impl Related<super::person_auth_google::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::PersonAuthGoogle.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
