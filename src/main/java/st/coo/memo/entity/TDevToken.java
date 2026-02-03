package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.KeyType;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;


@Setter
@Getter
@Table(value = "t_dev_token")
public class TDevToken implements Serializable {

    
    @Id(keyType = KeyType.Auto)
    private Integer id;

    
    private String name;

    
    private String token;

    
    private Integer userId;

}
