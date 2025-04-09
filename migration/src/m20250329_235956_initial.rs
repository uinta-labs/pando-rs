use sea_orm_migration::{prelude::*, sea_orm::Schema};

use entity::device::Entity as Device;
use entity::device_schedule::Entity as DeviceSchedule;
use entity::fleet::Entity as Fleet;
use entity::organization::Entity as Organization;
use entity::person::Entity as Person;
use entity::person_auth_google::Entity as PersonAuthGoogle;
use entity::person_organization::Entity as PersonOrganization;
use entity::schedule::Entity as Schedule;
use entity::waiting_room::Entity as WaitingRoom;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let builder = manager.get_database_backend();
        let schema = Schema::new(builder);

        // manager
        //     .create_table(
        // Example
        // Table::create()
        //     .table(Post::Table)
        //     .if_not_exists()
        //     .col(pk_auto(Post::Id))
        //     .col(string(Post::Title))
        //     .col(string(Post::Text))
        //     .to_owned(),
        // )
        // .await

        manager
            .create_table(schema.create_table_from_entity(Organization))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(Fleet))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(Person))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(PersonAuthGoogle))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(PersonOrganization))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(Device))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(Schedule))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(DeviceSchedule))
            .await?;

        manager
            .create_table(schema.create_table_from_entity(WaitingRoom))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Example
        // manager
        //     .drop_table(Table::drop().table(Post::Table).to_owned())
        //     .await

        manager
            .drop_table(Table::drop().table(PersonOrganization).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(PersonAuthGoogle).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Person).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Fleet).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Organization).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(DeviceSchedule).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Schedule).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Device).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WaitingRoom).to_owned())
            .await?;

        Ok(())
    }
}

// #[derive(DeriveIden)]
// enum Post {
//     Table,

// #[derive(DeriveIden)]
// enum Post {
//     Table,
//     Id,
//     Title,
//     Text,
// }
