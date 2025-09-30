#[async_trait::async_trait]
pub trait ResultExt<T, E>
where
    Self: Sized,
    T: Send,
{
    async fn map_err_async<F, Fut>(self, f: impl Send + FnOnce(E) -> Fut) -> Result<T, F>
    where
        F: Send,
        Fut: Send + Future<Output = F>;

    async fn or_else_async<F, Fut>(self, f: impl Send + FnOnce(E) -> Fut) -> Result<T, F>
    where
        F: Send,
        Fut: Send + Future<Output = Result<T, F>>;

    async fn inspect_err_async<Fut>(
        self,
        f: impl Send + for<'a> FnOnce(&'a E) -> Fut,
    ) -> Result<T, E>
    where
        Fut: Send + Future<Output = ()>;
}

#[async_trait::async_trait]
impl<T, E> ResultExt<T, E> for Result<T, E>
where
    T: Send,
    E: Sync + Send + 'static,
{
    async fn map_err_async<F, Fut>(self, f: impl Send + FnOnce(E) -> Fut) -> Result<T, F>
    where
        F: Send,
        Fut: Send + Future<Output = F>,
    {
        match self {
            Ok(x) => Ok(x),
            Err(e) => Err(f(e).await),
        }
    }

    async fn or_else_async<F, Fut>(self, f: impl Send + FnOnce(E) -> Fut) -> Result<T, F>
    where
        F: Send,
        Fut: Send + Future<Output = Result<T, F>>,
    {
        match self {
            Ok(x) => Ok(x),
            Err(e) => f(e).await,
        }
    }

    async fn inspect_err_async<Fut>(
        self,
        f: impl Send + for<'a> FnOnce(&'a E) -> Fut,
    ) -> Result<T, E>
    where
        Fut: Send + Future<Output = ()>,
    {
        match self {
            Ok(x) => Ok(x),
            Err(e) => {
                f(&e).await;
                Err(e)
            }
        }
    }
}
